//! 阻塞文件后端与可独立测试的同步协议。I/O 在 Library 的锁外，数据库阶段单独提交。
mod compaction;
mod model;
mod paths;
mod presence;
mod tree;
pub use presence::{StickerSyncInfo, platform};
pub mod network;
pub mod storage;
mod store;

use crate::{Device, Error, Library, Result, time::unix_millis};
use model::{Action, Kind, Model, Object, Operation};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
use storage::{FolderStorage, Storage};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    pub id: String,
    pub created_at: i64,
}
impl Manifest {
    pub fn create(storage: &dyn Storage) -> Result<Self> {
        if storage.read(".sticker-manager/library.json")?.is_some()
            || storage.read("library.json")?.is_some()
        {
            return Err(Error::Sync(
                "同步仓库已经存在，请加入现有仓库或选择空路径".into(),
            ));
        }
        if storage.list("")?.iter().any(|f| !system_file(&f.key)) {
            return Err(Error::Sync("请选择空路径，或已有的同步仓库".into()));
        }
        let manifest = Self {
            format: 2,
            id: Uuid::new_v4().to_string(),
            created_at: unix_millis()?,
        };
        storage.write(
            ".sticker-manager/library.json",
            &serde_json::to_vec(&manifest)?,
        )?;
        Ok(manifest)
    }
    pub fn read(storage: &dyn Storage) -> Result<Self> {
        Self::read_complete(storage)?
            .ok_or_else(|| Error::Sync("library.json 尚未完整送达，请稍后重试".into()))
    }
    /// 读到半截返回 None，等下一轮；文件不存在则报错，否则仓库被删后会一直显示已同步、改动却推不出去。
    fn read_complete(storage: &dyn Storage) -> Result<Option<Self>> {
        let Some(bytes) = storage.read(".sticker-manager/library.json")? else {
            if storage.read("library.json")?.is_some() {
                return Err(Error::Sync("旧同步格式，请重新创建同步仓库".into()));
            }
            return Err(Error::Sync(
                "存储目标里找不到 library.json：可能还没送达，或仓库已被删除或移走".into(),
            ));
        };
        let Ok(manifest) = serde_json::from_slice::<Self>(&bytes) else {
            return Ok(None);
        };
        if manifest.format == 1 {
            return Err(Error::Sync("旧同步格式，请重新创建同步仓库".into()));
        }
        if manifest.format > 2 {
            return Err(Error::SyncUpgrade(format!("同步格式 {}", manifest.format)));
        }
        if manifest.format != 2 || !valid_device(&manifest.id) {
            return Err(Error::Sync("无效的 library.json".into()));
        }
        Ok(Some(manifest))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDevice {
    pub id: String,
    pub name: String,
    pub last_sync_at: i64,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub received: BTreeMap<String, i64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInfo {
    pub manifest: Option<Manifest>,
    pub devices: usize,
    pub stickers: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderInfo {
    pub path: PathBuf,
    #[serde(flatten)]
    pub target: TargetInfo,
}
impl std::ops::Deref for FolderInfo {
    type Target = TargetInfo;
    fn deref(&self) -> &TargetInfo {
        &self.target
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    pub stage: TransferStage,
    pub completed: usize,
    /// 下载没有总数：原图按日志段分批出现，读完全部段之前不知道一共要下多少张。
    pub total: Option<usize>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TransferStage {
    Uploading,
    Downloading,
}
#[derive(Debug)]
pub struct SyncReport {
    pub changed: bool,
    pub last_sync_at: Option<i64>,
    pub devices: Vec<RemoteDevice>,
    pub pending: usize,
    pub new_stickers: usize,
}

/// 验证双向路径包含关系，canonicalize 也覆盖符号链接入口。
pub fn inspect_folder(path: &Path, local_root: &Path) -> Result<FolderInfo> {
    let storage = FolderStorage::new(path)?;
    let root = local_root.canonicalize()?;
    if storage.root().starts_with(&root) || root.starts_with(storage.root()) {
        return Err(Error::Sync(
            "同步文件夹不能是本机仓库、仓库里的文件夹，或包含仓库的文件夹".into(),
        ));
    }
    Ok(FolderInfo {
        path: storage.root().to_owned(),
        target: inspect(&storage)?,
    })
}

/// 所有后端共用的目标检查；系统文件和探测文件不算用户内容。
pub fn inspect(storage: &dyn Storage) -> Result<TargetInfo> {
    if storage.read(".sticker-manager/library.json")?.is_none() {
        if storage.read("library.json")?.is_some() {
            return Err(Error::Sync("旧同步格式，请重新创建同步仓库".into()));
        }
        if storage.list("")?.iter().any(|f| !system_file(&f.key)) {
            return Err(Error::Sync(
                "这个路径非空且没有 library.json；请选择空路径或已有的同步仓库".into(),
            ));
        }
        return Ok(TargetInfo {
            manifest: None,
            devices: 0,
            stickers: 0,
        });
    }
    let manifest = Manifest::read(storage)?;
    let mut model = Model::default();
    let mut waiting = Vec::new();
    let mut available = BTreeSet::new();
    let mut progress = BTreeMap::<String, i64>::new();
    for segment in segments(storage)? {
        let cursor = *progress.get(&segment.device).unwrap_or(&0);
        if segment.first > cursor + 1 || segment.last <= cursor {
            continue;
        }
        let Some(ops) = read_segment(storage, &segment)? else {
            continue;
        };
        for op in ops.into_iter().filter(|o| o.seq > cursor) {
            if !op.change.known() {
                return Err(Error::SyncUpgrade("不认识的操作类型".into()));
            }
            let mut objects = BTreeMap::new();
            op.change.objects(&mut objects);
            available.extend(objects.into_keys());
            waiting.extend(store::flatten(&op.change));
            // 没有下载原图：这里只统计已知元数据，文件延迟不影响加入确认。
            progress.insert(segment.device.clone(), op.seq);
        }
    }
    loop {
        let before = waiting.len();
        let mut next = Vec::new();
        for c in waiting {
            if !model.apply(&c, &available)? {
                next.push(c);
            }
        }
        waiting = next;
        if waiting.len() == before {
            break;
        }
    }
    // 预览不写 merge；按哈希统计可能尚未合并的同图。
    let stickers = model
        .projected(Kind::Sticker)
        .values()
        .filter_map(|e| e.data.as_ref().map(|d| d.hash.clone()))
        .collect::<BTreeSet<_>>()
        .len();
    Ok(TargetInfo {
        manifest: Some(manifest),
        devices: read_devices(storage)?.len(),
        stickers,
    })
}

#[derive(Debug, Clone)]
struct Segment {
    key: String,
    device: String,
    first: i64,
    last: i64,
    size: u64,
}
/// 访达的 .DS_Store、Windows 的 desktop.ini 这类系统自动放进去的文件，不算文件夹里有内容。
fn system_file(key: &str) -> bool {
    key.split('/').any(|part| {
        part.starts_with('.')
            || part == "Icon\r"
            || part.eq_ignore_ascii_case("desktop.ini")
            || part.eq_ignore_ascii_case("Thumbs.db")
    })
}
fn valid_device(s: &str) -> bool {
    Uuid::parse_str(s).is_ok_and(|id| id.to_string() == s)
}
fn segment(key: &str) -> Option<Segment> {
    let relative = key.strip_prefix(".sticker-manager/")?;
    let p = relative.split('/').collect::<Vec<_>>();
    if p.len() != 3 || p[0] != "ops" || !valid_device(p[1]) {
        return None;
    }
    let (first, last) = p[2].strip_suffix(".jsonl")?.split_once('-')?;
    if first.len() != 12
        || last.len() != 12
        || !first
            .bytes()
            .chain(last.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let first = first.parse().ok()?;
    let last = last.parse().ok()?;
    if first < 1 || last < first {
        return None;
    }
    Some(Segment {
        key: key.into(),
        device: p[1].into(),
        first,
        last,
        size: 0,
    })
}
fn segments(storage: &dyn Storage) -> Result<Vec<Segment>> {
    let mut result = storage
        .list(".sticker-manager/ops/")?
        .into_iter()
        .filter_map(|f| {
            let mut s = segment(&f.key)?;
            s.size = f.size;
            Some(s)
        })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| (&a.device, a.first, a.last).cmp(&(&b.device, b.first, b.last)));
    Ok(result)
}
fn read_segment(storage: &dyn Storage, segment: &Segment) -> Result<Option<Vec<Operation>>> {
    let Some(bytes) = storage.read(&segment.key)? else {
        return Ok(None);
    };
    Ok(decode_segment(&bytes, segment))
}
fn decode_segment(bytes: &[u8], segment: &Segment) -> Option<Vec<Operation>> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return None;
    };
    // 尾换行属于提交格式，截断但恰好能解析的最后一行也不会提前应用。
    if !text.ends_with('\n') {
        return None;
    }
    let mut result = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let Ok(operation) = serde_json::from_str::<Operation>(line) else {
            return None;
        };
        if operation.seq != segment.first + index as i64
            || operation.change.stamp.device != segment.device
        {
            return None;
        }
        result.push(operation);
    }
    if result.last().map(|o| o.seq) != Some(segment.last) {
        return None;
    }
    Some(result)
}
fn read_devices(storage: &dyn Storage) -> Result<Vec<RemoteDevice>> {
    let mut result = Vec::new();
    for file in storage.list(".sticker-manager/devices/")? {
        let Some(id) = file
            .key
            .strip_prefix(".sticker-manager/devices/")
            .and_then(|s| s.strip_suffix(".json"))
            .filter(|s| valid_device(s))
        else {
            continue;
        };
        if let Some(bytes) = storage.read(&file.key)?
            && let Ok(device) = serde_json::from_slice::<RemoteDevice>(&bytes)
            && device.id == id
        {
            result.push(device);
        }
    }
    Ok(result)
}
pub(crate) fn validate_sticker_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::EmptyName("表情"));
    }
    if name.contains(['/', '\\']) {
        return Err(Error::Sync("表情名称不能包含斜杠或反斜杠".into()));
    }
    if name.chars().any(char::is_control) {
        return Err(Error::Sync("表情名称不能包含控制字符".into()));
    }
    if name.trim().starts_with('.') || name.contains("..") {
        return Err(Error::Sync("表情名称不能以点开头，也不能包含 ..".into()));
    }
    Ok(())
}

fn lock(library: &Mutex<Library>) -> Result<std::sync::MutexGuard<'_, Library>> {
    library
        .lock()
        .map_err(|_| Error::Sync("仓库暂时无法访问，请重新启动应用".into()))
}

/// 在分配快照序号前检查身份，避免第一次加入恰好掩盖远端的 seq 1。
pub fn join(library: &Mutex<Library>, storage: &dyn Storage, id: &str) -> Result<()> {
    let remote = segments(storage)?;
    let mut lib = lock(library)?;
    let next: i64 = lib
        .conn
        .query_row(
            "SELECT next_seq FROM sync_targets WHERE library_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    if remote
        .iter()
        .any(|s| s.device == lib.device.id && s.last > next)
    {
        return Err(Error::DeviceConflict);
    }
    lib.join_sync(id)
}

/// 同步期间所有 Storage 调用、原图校验及缩略图生成都在 Mutex 外进行。
pub fn synchronize(
    library: &Mutex<Library>,
    storage: &dyn Storage,
    expected_id: &str,
) -> Result<SyncReport> {
    synchronize_with_progress(library, storage, expected_id, &|_| {})
}

pub fn synchronize_with_progress(
    library: &Mutex<Library>,
    storage: &dyn Storage,
    expected_id: &str,
    progress: &dyn Fn(SyncProgress),
) -> Result<SyncReport> {
    synchronize_with_events(library, storage, expected_id, progress, &|_| {})
}

/// 新表情在提交后、仓库锁外通知，外壳按到达当时的窗口可见状态决定是否累计角标。
pub fn synchronize_with_events(
    library: &Mutex<Library>,
    storage: &dyn Storage,
    expected_id: &str,
    progress: &dyn Fn(SyncProgress),
    arrived: &dyn Fn(usize),
) -> Result<SyncReport> {
    synchronize_with_maintenance(library, storage, expected_id, progress, arrived, &|| true)
}

/// 合并许可在每次维护 I/O 前重新检查；手动同步也不能绕过计费网络限制。
pub fn synchronize_with_maintenance(
    library: &Mutex<Library>,
    storage: &dyn Storage,
    expected_id: &str,
    progress: &dyn Fn(SyncProgress),
    arrived: &dyn Fn(usize),
    maintenance_allowed: &dyn Fn() -> bool,
) -> Result<SyncReport> {
    let Some(manifest) = Manifest::read_complete(storage)? else {
        return Ok(SyncReport {
            changed: false,
            last_sync_at: None,
            devices: Vec::new(),
            pending: 1,
            new_stickers: 0,
        });
    };
    if manifest.id != expected_id {
        return Err(Error::Sync(
            "目标中的仓库已更换，请重新选择存储目标并确认加入".into(),
        ));
    }
    let remote = segments(storage)?;
    let (root, device, files, next) = {
        let lib = lock(library)?;
        let next = lib
            .conn
            .query_row(
                "SELECT next_seq FROM sync_targets WHERE library_id=?1",
                [expected_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0);
        (
            lib.root.clone(),
            lib.device.clone(),
            lib.files.clone(),
            next,
        )
    };
    if remote
        .iter()
        .any(|s| s.device == device.id && s.last > next)
    {
        return Err(Error::DeviceConflict);
    }
    let exists = {
        lock(library)?.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_targets WHERE library_id=?1)",
            [expected_id],
            |r| r.get::<_, bool>(0),
        )?
    };
    if !exists {
        lock(library)?.join_sync(expected_id)?;
    }
    let initial_complete: bool = lock(library)?.conn.query_row(
        "SELECT initial_pull_complete FROM sync_targets WHERE library_id=?1",
        [expected_id],
        |r| r.get(0),
    )?;
    let mut new_stickers = 0;
    let downloaded = std::cell::Cell::new(0);
    let on_download = |finished: bool| {
        if finished {
            downloaded.set(downloaded.get() + 1);
        }
        progress(SyncProgress {
            stage: TransferStage::Downloading,
            completed: downloaded.get(),
            total: None,
        });
    };
    let mut published = Vec::new();
    let cursors = {
        let lib = lock(library)?;
        remote
            .iter()
            .map(|s| {
                Ok((
                    s.device.clone(),
                    if s.device == device.id {
                        next
                    } else {
                        store::cursor(&lib.conn, expected_id, &s.device)?
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?
    };
    let batches = remote
        .iter()
        .map(|s| {
            Ok((
                s.clone(),
                if s.last <= cursors[&s.device] {
                    Some(Vec::new())
                } else {
                    read_segment(storage, s)?
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut scan = tree::scan(
        library,
        storage,
        expected_id,
        &root,
        &batches,
        cursors,
        &on_download,
    )?;
    let mut changed = false;
    let mut first_error = None;
    let mut blocked = BTreeSet::new();
    for (s, operations) in &batches {
        if s.device == device.id || blocked.contains(&s.device) {
            continue;
        }
        let cursor = { store::cursor(&lock(library)?.conn, expected_id, &s.device)? };
        if s.last <= cursor || s.first > cursor + 1 {
            continue;
        }
        let Some(mut ops) = operations.clone() else {
            continue;
        };
        ops.retain(|o| o.seq > cursor);
        if let Some(index) = ops.iter().position(|o| !o.change.known()) {
            ops.truncate(index);
            blocked.insert(s.device.clone());
            first_error = Some(Error::SyncUpgrade("不认识的操作类型".into()));
        }
        if ops.is_empty() {
            continue;
        }
        let mut objects = BTreeMap::new();
        for op in &ops {
            op.change.objects(&mut objects);
        }
        let available = download_objects(storage, &root, &files, &objects, &scan, &on_download)?;
        let mut lib = lock(library)?;
        let tx = lib.conn.transaction()?;
        let (mut revision, mut model) = store::load(&tx)?;
        store::flush(&tx, &mut model, &mut revision)?;
        let known = presence::known_hashes(&model);
        for op in &ops {
            tx.execute(
                "UPDATE sync_meta SET max_at=MAX(max_at,?1)",
                [op.change.max_at()],
            )?;
            let mut waiting = Vec::new();
            for change in store::flatten(&op.change) {
                if !model.apply(&change, &available)? {
                    waiting.push(change);
                } else {
                    presence::record_origin(&tx, expected_id, &s.device, op.seq, &change)?;
                }
            }
            store::pending(&tx, expected_id, &s.device, op.seq, waiting)?;
        }
        store::retry(&tx, &mut model, &available)?;
        store::deduplicate(&tx, &mut model, &device.id)?;
        let added = presence::new_stickers(&known, &model);
        new_stickers += added;
        store::project(&tx, &model)?;
        store::save(&tx, revision, &model)?;
        store::prune(&tx, Some(expected_id))?;
        tx.execute("INSERT INTO sync_progress VALUES(?1,?2,?3) ON CONFLICT(library_id,device) DO UPDATE SET seq=excluded.seq",params![expected_id,s.device,ops.last().unwrap().seq])?;
        tx.commit()?;
        drop(lib);
        if initial_complete && added > 0 {
            arrived(added);
        }
        changed = true;
    }
    // 日志已接收但原图或跨设备依赖晚到：即使没有新段，也要重试。
    let objects = {
        let lib = lock(library)?;
        let mut result = BTreeMap::new();
        for (_, _, _, cs) in store::pending_rows(&lib.conn)? {
            for c in cs {
                c.objects(&mut result);
            }
        }
        result
    };
    let available = download_objects(storage, &root, &files, &objects, &scan, &on_download)?;
    {
        let mut lib = lock(library)?;
        let tx = lib.conn.transaction()?;
        let (mut revision, mut model) = store::load(&tx)?;
        store::flush(&tx, &mut model, &mut revision)?;
        let before = model.clone();
        let known = presence::known_hashes(&model);
        store::retry(&tx, &mut model, &available)?;
        store::deduplicate(&tx, &mut model, &device.id)?;
        let added = presence::new_stickers(&known, &model);
        new_stickers += added;
        if model != before {
            store::project(&tx, &model)?;
            changed = true;
        }
        store::save(&tx, revision, &model)?;
        store::prune(&tx, Some(expected_id))?;
        tx.commit()?;
        drop(lib);
        if initial_complete && added > 0 {
            arrived(added);
        }
    }
    if scan.ready {
        let known = presence::known_hashes(&store::load(&lock(library)?.conn)?.1);
        changed |= tree::reconcile(library, expected_id, &mut scan)?;
        let added = presence::new_stickers(&known, &store::load(&lock(library)?.conn)?.1);
        new_stickers += added;
        if initial_complete && added > 0 {
            arrived(added);
        }
        tree::push_files(library, storage, expected_id, &root, progress)?;
    }
    changed |= push(
        library,
        storage,
        expected_id,
        &root,
        &device,
        progress,
        &mut published,
    )?;
    tree::purge(library, storage, expected_id)?;
    cleanup_purged_files(library, &root, &files)?;
    if let Some(error) = first_error {
        return Err(error);
    }
    let last_sync_at = unix_millis()?;
    let received = presence::received(&lock(library)?.conn, expected_id)?;
    storage.write(
        &format!(".sticker-manager/devices/{}.json", device.id),
        &serde_json::to_vec(&RemoteDevice {
            id: device.id.clone(),
            name: device.name,
            last_sync_at,
            platform: platform().into(),
            received,
        })?,
    )?;
    let mut pending =
        lock(library)?
            .conn
            .query_row("SELECT COUNT(*) FROM sync_pending", [], |r| {
                r.get::<_, i64>(0)
            })? as usize;
    pending += lock(library)?.conn.query_row(
        "SELECT COUNT(*) FROM sync_file_moves WHERE library_id=?1",
        [expected_id],
        |r| r.get::<_, i64>(0),
    )? as usize;
    pending += usize::from(!scan.ready);
    let devices = read_devices(storage)?;
    {
        let lib = lock(library)?;
        let tx = lib.conn.unchecked_transaction()?;
        tx.execute("INSERT INTO sync_device_cache VALUES(?1,?2,?3) ON CONFLICT(library_id) DO UPDATE SET devices=excluded.devices,last_sync_at=excluded.last_sync_at", params![expected_id, serde_json::to_string(&devices)?, last_sync_at])?;
        if pending == 0 {
            tx.execute(
                "UPDATE sync_targets SET initial_pull_complete=1 WHERE library_id=?1",
                [expected_id],
            )?;
        }
        tx.commit()?;
    }
    let pulled = {
        let lib = lock(library)?;
        remote
            .iter()
            .filter(|s| s.device != device.id)
            .try_fold(true, |complete, s| {
                Ok::<_, Error>(
                    complete && store::cursor(&lib.conn, expected_id, &s.device)? >= s.last,
                )
            })?
    };
    if pulled && pending == 0 && maintenance_allowed() {
        let own = remote
            .into_iter()
            .chain(published)
            .filter(|s| s.device == device.id)
            .collect();
        if let Err(error) = compaction::compact(storage, own, maintenance_allowed) {
            eprintln!("合并远端操作日志失败：{error}");
        }
    }
    Ok(SyncReport {
        changed,
        last_sync_at: Some(last_sync_at),
        devices,
        pending,
        new_stickers: if initial_complete { new_stickers } else { 0 },
    })
}

fn push(
    library: &Mutex<Library>,
    storage: &dyn Storage,
    id: &str,
    _root: &Path,
    device: &Device,
    _progress: &dyn Fn(SyncProgress),
    published: &mut Vec<Segment>,
) -> Result<bool> {
    let (ops, changed) = {
        let mut lib = lock(library)?;
        let tx = lib.conn.transaction()?;
        let (mut revision, mut model) = store::load(&tx)?;
        store::flush(&tx, &mut model, &mut revision)?;
        let before = model.clone();
        store::deduplicate(&tx, &mut model, &device.id)?;
        let changed = model != before;
        if changed {
            store::project(&tx, &model)?;
        }
        store::save(&tx, revision, &model)?;
        let last = tx.query_row(
            "SELECT revision FROM sync_targets WHERE library_id=?1",
            [id],
            |r| r.get(0),
        )?;
        let changes = store::local_changes(&tx, last)?;
        // 使用操作在分配传输 seq 前合并；序列保持连续，不产生人为缺口。
        let mut last_usage = BTreeMap::new();
        for (i, (_, c)) in changes.iter().enumerate() {
            if let Action::Usage { sticker, owner, .. } = &c.action {
                last_usage.insert((sticker, owner), i);
            }
        }
        for (i, (_, change)) in changes.iter().enumerate() {
            if let Action::Usage { sticker, owner, .. } = &change.action
                && last_usage.get(&(sticker, owner)) != Some(&i)
            {
                continue;
            }
            store::enqueue(&tx, id, change.clone())?;
        }
        if let Some((revision, _)) = changes.last() {
            tx.execute(
                "UPDATE sync_targets SET revision=?2 WHERE library_id=?1",
                params![id, revision],
            )?;
        }
        store::prune(&tx, Some(id))?;
        let ops = {
            let mut q=tx.prepare("SELECT operation FROM sync_stream WHERE library_id=?1 AND seq>(SELECT pushed_seq FROM sync_targets WHERE library_id=?1) ORDER BY seq")?;
            let rows = q
                .query_map([id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter()
                .map(|s| serde_json::from_str::<Operation>(&s))
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        tx.commit()?;
        (ops, changed)
    };
    for batch in ops.chunks(128) {
        let first = batch.first().unwrap().seq;
        let last = batch.last().unwrap().seq;
        let key = format!(
            ".sticker-manager/ops/{}/{first:012}-{last:012}.jsonl",
            device.id
        );
        let mut bytes = Vec::new();
        for op in batch {
            serde_json::to_writer(&mut bytes, op)?;
            bytes.push(b'\n');
        }
        if storage.read(&key)?.as_deref() != Some(bytes.as_slice()) {
            storage.write(&key, &bytes)?;
        }
        published.push(Segment {
            key,
            device: device.id.clone(),
            first,
            last,
            size: bytes.len() as u64,
        });
        let mut lib = lock(library)?;
        let tx = lib.conn.transaction()?;
        tx.execute(
            "UPDATE sync_targets SET pushed_seq=MAX(pushed_seq,?2) WHERE library_id=?1",
            params![id, last],
        )?;
        store::prune(&tx, Some(id))?;
        tx.commit()?;
    }
    Ok(changed)
}

fn download_objects(
    storage: &dyn Storage,
    root: &Path,
    files: &Mutex<()>,
    objects: &BTreeMap<String, Object>,
    scan: &tree::Scan,
    downloaded: &dyn Fn(bool),
) -> Result<BTreeSet<String>> {
    let mut available = BTreeSet::new();
    for (key, object) in objects {
        if !object.validate() {
            return Err(Error::Sync("原图元数据不合法".into()));
        }
        let path = root.join(key);
        let local = match fs::read(&path) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let thumbnail = root.join("thumbs").join(format!("{}.png", object.hash));
        let (bytes, fetched) = match local {
            Some(b) if format!("{:x}", Sha256::digest(&b)) == object.hash => {
                // 本机已有这张原图（比如两边导入过同一张），缩略图也在，就不用重新解码。
                if thumbnail.is_file() {
                    available.insert(key.clone());
                    continue;
                }
                (Some(b), false)
            }
            _ => {
                downloaded(false);
                (
                    match scan.current.values().find(|e| e.object.hash == object.hash) {
                        Some(entry) => storage.read(&entry.file.key)?,
                        None => None,
                    },
                    true,
                )
            }
        };
        let Some(bytes) = bytes else { continue };
        if format!("{:x}", Sha256::digest(&bytes)) != object.hash {
            continue;
        }
        let mut file = tempfile::NamedTempFile::new_in(root.join("objects"))?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        let format = image::guess_format(&bytes)?;
        let (image, _) = crate::import::decode(file.path(), format)?;
        let mut thumb = tempfile::NamedTempFile::new_in(root.join("thumbs"))?;
        image
            .thumbnail(
                image.width().min(crate::import::THUMBNAIL_SIZE),
                image.height().min(crate::import::THUMBNAIL_SIZE),
            )
            .write_to(&mut thumb, image::ImageFormat::Png)?;
        thumb.as_file().sync_all()?;
        let _files = files.lock().unwrap();
        file.persist(path).map_err(|e| e.error)?;
        thumb.persist(thumbnail).map_err(|e| e.error)?;
        available.insert(key.clone());
        if fetched {
            downloaded(true);
        }
    }
    Ok(available)
}

fn cleanup_purged_files(library: &Mutex<Library>, root: &Path, files: &Mutex<()>) -> Result<()> {
    let candidates = {
        let lib = lock(library)?;
        let (_, model) = store::load(&lib.conn)?;
        model
            .entities
            .get(&Kind::Sticker)
            .into_iter()
            .flat_map(|entities| entities.iter())
            .filter(|(id, _)| model.dead(Kind::Sticker, id))
            .filter_map(|(_, entity)| entity.data.clone())
            .collect::<Vec<_>>()
    };
    for object in candidates {
        let _files = files.lock().unwrap();
        // 导入按 Library -> files 加锁；这里不能反向等待 Library，忙时留到下一轮。
        let Ok(lib) = library.try_lock() else {
            continue;
        };
        let live: bool = lib.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM stickers WHERE hash=?1)",
            [&object.hash],
            |r| r.get(0),
        )?;
        drop(lib);
        if !live {
            let _ = fs::remove_file(root.join(object.key()));
            let _ = fs::remove_file(root.join("thumbs").join(format!("{}.png", object.hash)));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
