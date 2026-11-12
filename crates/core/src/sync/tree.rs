//! 目录扫描、仅本机的路径缓存，以及可重试的复制后删除。
use super::*;
use serde_json::json;
use storage::FileEntry;

#[derive(Clone)]
pub(super) struct Entry {
    pub file: FileEntry,
    pub id: String,
    pub object: Object,
    pub managed: bool,
}
pub(super) struct Scan {
    pub old: BTreeMap<String, Entry>,
    pub current: BTreeMap<String, Entry>,
    pub ready: bool,
    pub folders: BTreeSet<String>,
    pub moving: BTreeSet<String>,
    /// 本轮扫描时列出的全部远端条目，`push_files` 先用它试算，免得空闲轮次再列一次。
    pub listed: BTreeMap<String, FileEntry>,
}
fn cached(library: &Mutex<Library>, target: &str) -> Result<BTreeMap<String, Entry>> {
    let lib = lock(library)?;
    let mut q = lib.conn.prepare(
        "SELECT path,size,version,sticker_id,object,managed FROM sync_path_cache WHERE library_id=?1",
    )?;
    let rows = q
        .query_map([target], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)? as u64,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, bool>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(key, size, version, id, object, managed)| {
            Ok((
                key.clone(),
                Entry {
                    file: FileEntry { key, size, version },
                    id,
                    object: serde_json::from_str(&object)?,
                    managed,
                },
            ))
        })
        .collect()
}
fn save_entry(conn: &rusqlite::Connection, target: &str, entry: &Entry) -> Result<()> {
    conn.execute("INSERT INTO sync_path_cache VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(library_id,path) DO UPDATE SET size=excluded.size,version=excluded.version,hash=excluded.hash,sticker_id=excluded.sticker_id,object=excluded.object,managed=excluded.managed",params![target,entry.file.key,entry.file.size as i64,entry.file.version,entry.object.hash,entry.id,serde_json::to_string(&entry.object)?,entry.managed])?;
    Ok(())
}
fn trash(path: &str) -> bool {
    path.starts_with(".sticker-manager/trash/")
}
fn winners(entries: &BTreeMap<String, Entry>) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    // 正常目录按 UTF-8 字节排序；已进入 trash 时，其他正常路径只是忽略的副本。
    for (path, entry) in entries
        .iter()
        .filter(|(p, _)| trash(p))
        .chain(entries.iter().filter(|(p, _)| !trash(p)))
    {
        result
            .entry(entry.object.hash.clone())
            .or_insert_with(|| path.clone());
    }
    result
}

pub(super) fn scan(
    library: &Mutex<Library>,
    storage: &dyn Storage,
    target: &str,
    root: &Path,
    remote: &[(Segment, Option<Vec<Operation>>)],
    mut cursors: BTreeMap<String, i64>,
    downloaded: &dyn Fn(bool),
) -> Result<Scan> {
    let old = cached(library, target)?;
    let files = lock(library)?.files.clone();
    let (mut hints, moving) = {
        let lib = lock(library)?;
        let (_, model) = store::load(&lib.conn)?;
        let mut q = lib
            .conn
            .prepare("SELECT source,destination FROM sync_file_moves WHERE library_id=?1")?;
        let rows = q
            .query_map([target], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        (
            model,
            rows.into_iter()
                .flat_map(|(a, b)| [a, b])
                .collect::<BTreeSet<_>>(),
        )
    };
    // 路径声明只帮助找原图，不应用字段，也不推进接收游标。
    for (segment, operations) in remote {
        let cursor = *cursors.get(&segment.device).unwrap_or(&0);
        if segment.first > cursor + 1 || segment.last <= cursor {
            continue;
        }
        let Some(ops) = operations.clone() else {
            continue;
        };
        for op in ops.into_iter().filter(|o| o.seq > cursor) {
            if !op.change.known() {
                break;
            }
            for c in store::flatten(&op.change) {
                if let Action::File {
                    id,
                    path,
                    data,
                    version,
                } = c.action
                {
                    hints.record_file(&id, &path, &data, &c.stamp, &version)?;
                }
            }
            cursors.insert(segment.device.clone(), op.seq);
        }
    }
    let ready = remote
        .iter()
        .all(|(s, _)| cursors.get(&s.device).copied().unwrap_or(0) >= s.last);
    let mut by_path = BTreeMap::<String, (String, model::Location)>::new();
    for (id, history) in &hints.file_history {
        for location in history.iter() {
            if by_path
                .get(&location.path)
                .is_none_or(|(_, old)| location.stamp > old.stamp)
            {
                by_path.insert(location.path.clone(), (id.clone(), location.clone()));
            }
        }
    }
    let mut identities = old
        .values()
        .map(|e| (e.object.hash.clone(), (e.id.clone(), e.object.clone())))
        .collect::<BTreeMap<_, _>>();
    for (id, e) in hints.projected(Kind::Sticker) {
        if let Some(o) = e.data {
            identities.insert(o.hash.clone(), (id, o));
        }
    }
    let mut current = BTreeMap::new();
    let mut folders = BTreeSet::new();
    let files_listed = storage.list("")?;
    let listed = files_listed
        .iter()
        .map(|f| (f.key.clone(), f.clone()))
        .collect::<BTreeMap<_, _>>();
    // 按存储返回的顺序处理，同一哈希的多份副本分配身份的先后不变。
    for file in files_listed {
        if let Some(folder) = file
            .key
            .strip_suffix('/')
            .filter(|p| paths::component(p) && *p != "未分类")
        {
            folders.insert(folder.to_owned());
        }
        if let Some((Some(folder), _, _)) = paths::image(&file.key) {
            folders.insert(folder.to_owned());
        }
        if paths::image(&file.key).is_none()
            && !(trash(&file.key)
                && (by_path.contains_key(&file.key) || old.contains_key(&file.key)))
        {
            continue;
        }
        if let Some(entry) = old
            .get(&file.key)
            .filter(|e| e.file.size == file.size && e.file.version == file.version)
        {
            current.insert(file.key.clone(), entry.clone());
            continue;
        }
        if !old.contains_key(&file.key)
            && let Some((id, location)) = by_path
                .get(&file.key)
                .filter(|(_, l)| l.data.byte_size as u64 == file.size && l.version == file.version)
        {
            current.insert(
                file.key.clone(),
                Entry {
                    file,
                    id: id.clone(),
                    object: location.data.clone(),
                    managed: false,
                },
            );
            continue;
        }
        downloaded(false);
        let Some(bytes) = storage.read(&file.key)? else {
            continue;
        };
        if bytes.len() as u64 != file.size {
            continue;
        }
        let mut tmp = tempfile::NamedTempFile::new_in(root.join("objects"))?;
        tmp.write_all(&bytes)?;
        let decoded = image::guess_format(&bytes)
            .ok()
            .and_then(|f| crate::import::decode(tmp.path(), f).ok().map(|d| (f, d)));
        let Some((format, (image, animated))) = decoded else {
            // 半截写入不成为删除，也不能覆盖本机已验证的原图。
            if let Some(entry) = old.get(&file.key) {
                current.insert(file.key.clone(), entry.clone());
            }
            continue;
        };
        let ext = match format {
            image::ImageFormat::Png => "png",
            image::ImageFormat::Jpeg => "jpg",
            image::ImageFormat::Gif => "gif",
            image::ImageFormat::WebP => "webp",
            _ => continue,
        };
        let object = Object {
            hash: format!("{:x}", Sha256::digest(&bytes)),
            ext: ext.into(),
            animated: i64::from(animated),
            width: image.width(),
            height: image.height(),
            byte_size: bytes.len() as i64,
            added_at: 0,
        };
        let thumb = crate::import::write_thumbnail(root, &image)?;
        tmp.as_file().sync_all()?;
        {
            let _files = files.lock().unwrap();
            tmp.persist(root.join(object.key())).map_err(|e| e.error)?;
            thumb
                .persist(root.join("thumbs").join(format!("{}.png", object.hash)))
                .map_err(|e| e.error)?;
        }
        let (id, object) = identities.get(&object.hash).cloned().unwrap_or_else(|| {
            let id = by_path
                .get(&file.key)
                .filter(|(_, l)| l.data.hash == object.hash)
                .map(|(id, _)| id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            identities.insert(object.hash.clone(), (id.clone(), object.clone()));
            (id, object)
        });
        current.insert(
            file.key.clone(),
            Entry {
                file,
                id,
                object,
                managed: false,
            },
        );
        downloaded(true);
    }
    Ok(Scan {
        old,
        current,
        moving,
        ready,
        folders,
        listed,
    })
}

fn category_name(entity: &model::Entity) -> Option<&str> {
    entity.fields.get("name").and_then(|r| r.value.as_str())
}
fn append_apply(
    conn: &rusqlite::Connection,
    model: &mut Model,
    device: &str,
    action: Action,
) -> Result<()> {
    let change = store::append(conn, device, action)?;
    let mut objects = BTreeMap::new();
    change.objects(&mut objects);
    model.apply(&change, &objects.into_keys().collect())?;
    Ok(())
}
fn add_category(
    conn: &rusqlite::Connection,
    model: &mut Model,
    device: &str,
    name: &str,
    position: &mut i64,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let colors = crate::organization::CATEGORY_COLORS;
    append_apply(
        conn,
        model,
        device,
        Action::Create {
            entity: Kind::Category,
            id: id.clone(),
            data: None,
            fields: serde_json::from_value(
                json!({"name":name,"color":colors[position.rem_euclid(colors.len() as i64) as usize],"position":*position}),
            )?,
        },
    )?;
    *position += 1;
    Ok(id)
}

pub(super) fn reconcile(library: &Mutex<Library>, target: &str, scan: &mut Scan) -> Result<bool> {
    let previous = winners(&scan.old);
    let chosen = winners(&scan.current);
    let mut lib = lock(library)?;
    let root = lib.root.clone();
    let device = lib.device.id.clone();
    let tx = lib.conn.transaction()?;
    let (mut revision, mut model) = store::load(&tx)?;
    store::flush(&tx, &mut model, &mut revision)?;
    let mut stickers = model.projected(Kind::Sticker);
    let categories = model.projected(Kind::Category);
    let category_reps = model.representatives(Kind::Category);
    let mut category_ids = categories
        .iter()
        .filter_map(|(id, e)| category_name(e).map(|name| (name.to_ascii_lowercase(), id.clone())))
        .collect::<BTreeMap<_, _>>();
    let mut position = categories
        .values()
        .filter_map(|e| e.fields.get("position").and_then(|r| r.value.as_i64()))
        .max()
        .unwrap_or(-1)
        + 1;
    let mut hashes = stickers
        .iter()
        .filter_map(|(id, e)| e.data.as_ref().map(|o| (o.hash.clone(), id.clone())))
        .collect::<BTreeMap<_, _>>();
    let local = {
        let mut q = tx.prepare("SELECT id,name,category_id FROM stickers")?;
        q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?),
            ))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?
    };
    let dirty = {
        let mut q = tx.prepare("SELECT sticker_id,revision FROM sync_file_dirty")?;
        q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()?
    };
    let directories = {
        let mut q =
            tx.prepare("SELECT path,category_id FROM sync_directory_cache WHERE library_id=?1")?;
        q.query_map([target], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?
    };
    let dead_names = model
        .entities
        .get(&Kind::Category)
        .into_iter()
        .flat_map(|e| e.iter())
        .filter(|(id, _)| model.dead(Kind::Category, id))
        .filter_map(|(_, e)| category_name(e).map(str::to_ascii_lowercase))
        .collect::<BTreeSet<_>>();
    let old_folders = scan
        .old
        .keys()
        .filter_map(|p| paths::image(p).and_then(|(c, _, _)| c).map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let mut projected = false;
    let mut model_changed = false;
    let mut renamed = BTreeMap::<String, BTreeSet<String>>::new();
    for (hash, oldpath) in &previous {
        if let (Some((Some(old), _, _)), Some((Some(new), _, _))) = (
            paths::image(oldpath),
            chosen.get(hash).and_then(|p| paths::image(p)),
        ) && old != new
            && !scan.folders.contains(old)
        {
            renamed.entry(old.into()).or_default().insert(new.into());
        }
    }
    for (old, names) in renamed {
        if names.len() != 1 {
            continue;
        }
        let name = names.into_iter().next().unwrap();
        let key = name.to_ascii_lowercase();
        if category_ids.contains_key(&key) {
            continue;
        }
        if let Some(id) = category_ids.remove(&old.to_ascii_lowercase()) {
            append_apply(
                &tx,
                &mut model,
                &device,
                Action::Set {
                    entity: Kind::Category,
                    id: id.clone(),
                    field: "name".into(),
                    value: json!(name),
                },
            )?;
            category_ids.insert(key, id);
            projected = true;
        }
    }
    for folder in &scan.folders {
        let key = folder.to_ascii_lowercase();
        if !directories.contains_key(folder)
            && !category_ids.contains_key(&key)
            && !(old_folders.contains(folder) && dead_names.contains(&key))
        {
            let id = add_category(&tx, &mut model, &device, folder, &mut position)?;
            category_ids.insert(key, id);
            projected = true;
        }
    }
    let sticker_reps = model.representatives(Kind::Sticker);
    let mut live_paths = BTreeMap::<String, model::Location>::new();
    for (owner, h) in &model.file_history {
        if let (Some(id), Some(location)) = (
            model.representative(&sticker_reps, Kind::Sticker, owner),
            h.live.as_ref(),
        ) && live_paths
            .get(&id)
            .is_none_or(|old| (&location.stamp, &location.path) > (&old.stamp, &old.path))
        {
            live_paths.insert(id, location.clone());
        }
    }
    let mut deleted_ids = stickers
        .iter()
        .filter(|(_, e)| {
            e.fields
                .get("deleted_at")
                .is_some_and(|r| !r.value.is_null())
        })
        .map(|(id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    let mut current_names = stickers
        .iter()
        .filter_map(|(id, e)| category_name(e).map(|name| (id.clone(), name.to_owned())))
        .collect::<BTreeMap<_, _>>();
    let mut assignments = BTreeMap::<String, Option<String>>::new();
    let mut clear = BTreeSet::new();
    let mut unclassified = BTreeSet::new();
    for (hash, path) in &chosen {
        if scan.moving.contains(path) {
            continue;
        }
        let entry = &scan.current[path];
        let id = if let Some(id) = hashes.get(hash) {
            id.clone()
        } else {
            if model.dead(Kind::Sticker, &entry.id) || !root.join(entry.object.key()).is_file() {
                continue;
            }
            let name = paths::image(path).map(|(_, n, _)| n).unwrap_or("");
            append_apply(
                &tx,
                &mut model,
                &device,
                Action::Create {
                    entity: Kind::Sticker,
                    id: entry.id.clone(),
                    data: Some(entry.object.clone()),
                    fields: serde_json::from_value(
                        json!({"name":name,"caption":null,"favorite":0,"deleted_at":null}),
                    )?,
                },
            )?;
            stickers.insert(
                entry.id.clone(),
                model.entities[&Kind::Sticker][&entry.id].clone(),
            );
            hashes.insert(hash.clone(), entry.id.clone());
            projected = true;
            entry.id.clone()
        };
        let location = if trash(path) {
            model.file_history.get(&id).and_then(|h| h.trash.as_ref())
        } else {
            model.file_history.get(&id).and_then(|h| h.live.as_ref())
        };
        // 只因 version 不同不重发：网盘同步后各设备看到的修改时间可能不同，重发会让两台设备来回改写日志。
        // 声明里的 version 只用于新设备判断能否沿用哈希，对不上时多下载一次即可。
        if location.is_none_or(|l| l.path != *path || l.data.hash != *hash) {
            append_apply(
                &tx,
                &mut model,
                &device,
                Action::File {
                    id: id.clone(),
                    path: path.clone(),
                    data: entry.object.clone(),
                    version: entry.file.version.clone(),
                },
            )?;
            model_changed = true;
            if !trash(path)
                && let Some(location) = model.file_history.get(&id).and_then(|h| h.live.clone())
            {
                live_paths.insert(id.clone(), location);
            }
        }
        let live_path = if trash(path) {
            live_paths.get(&id).map(|l| l.path.as_str()).unwrap_or(path)
        } else {
            path
        };
        let Some((category, name, _)) = paths::image(live_path) else {
            continue;
        };
        let remote_changed = previous.get(hash).is_some_and(|p| p != path);
        if !dirty.contains_key(&id) || remote_changed || !previous.contains_key(hash) || trash(path)
        {
            let category_id = if let Some(name) = category {
                let key = name.to_ascii_lowercase();
                if let Some(id) = category_ids.get(&key) {
                    Some(id.clone())
                } else if let Some(id) = directories
                    .get(name)
                    .and_then(|id| id.as_deref())
                    .and_then(|id| model.representative(&category_reps, Kind::Category, id))
                {
                    Some(id)
                } else if (previous.get(hash) == Some(path) || trash(path))
                    && (dead_names.contains(&key)
                        || directories
                            .get(name)
                            .and_then(|id| id.as_deref())
                            .is_some_and(|id| model.dead(Kind::Category, id)))
                {
                    None
                } else {
                    let id = add_category(&tx, &mut model, &device, name, &mut position)?;
                    category_ids.insert(key, id.clone());
                    projected = true;
                    Some(id)
                }
            } else {
                None
            };
            let old_name = stickers.get(&id).and_then(category_name);
            if old_name != Some(name) {
                append_apply(
                    &tx,
                    &mut model,
                    &device,
                    Action::Set {
                        entity: Kind::Sticker,
                        id: id.clone(),
                        field: "name".into(),
                        value: json!(name),
                    },
                )?;
                current_names.insert(id.clone(), name.into());
                projected = true;
            }
            assignments.insert(id.clone(), category_id.clone());
            if category.is_some() && category_id.is_none() {
                unclassified.insert(id.clone());
            } else {
                clear.insert(id.clone());
            }
        }
    }
    // 覆盖同一路径的另一张图也属于消失；不能把旧缓存的哈希套到新内容上。
    for (hash, oldpath) in &previous {
        if chosen.contains_key(hash) || scan.moving.contains(oldpath) || trash(oldpath) {
            continue;
        }
        if let Some(id) = hashes.get(hash)
            && stickers[id]
                .fields
                .get("deleted_at")
                .is_none_or(|r| r.value.is_null())
        {
            append_apply(
                &tx,
                &mut model,
                &device,
                Action::Set {
                    entity: Kind::Sticker,
                    id: id.clone(),
                    field: "deleted_at".into(),
                    value: json!(unix_millis()?),
                },
            )?;
            deleted_ids.insert(id.clone());
            projected = true;
        }
    }
    // 最近删除的名称和分类也统一由同一条正常路径声明推导，不保留各设备不同的旧投影。
    for id in &deleted_ids {
        let path = live_paths.get(id).and_then(|l| paths::image(&l.path));
        assignments.insert(
            id.clone(),
            path.and_then(|(c, _, _)| c)
                .and_then(|c| category_ids.get(&c.to_ascii_lowercase()).cloned()),
        );
        if let Some((_, name, _)) = path {
            let current = current_names.get(id).map(String::as_str);
            if current != Some(name) {
                append_apply(
                    &tx,
                    &mut model,
                    &device,
                    Action::Set {
                        entity: Kind::Sticker,
                        id: id.clone(),
                        field: "name".into(),
                        value: json!(name),
                    },
                )?;
                projected = true;
            }
        }
    }
    if projected {
        store::project(&tx, &model)?;
    }
    let mut changed = projected;
    tx.execute("UPDATE sync_meta SET recording=0", [])?;
    for (id, category) in assignments {
        if projected || local.get(&id).is_none_or(|(_, old)| old != &category) {
            changed |= tx.execute(
                "UPDATE stickers SET category_id=?2 WHERE id=?1 AND category_id IS NOT ?2",
                params![id, category],
            )? > 0;
        }
    }
    tx.execute("UPDATE sync_meta SET recording=1", [])?;
    for id in clear {
        tx.execute("DELETE FROM sync_file_dirty WHERE sticker_id=?1", [id])?;
    }
    for id in unclassified {
        tx.execute("INSERT INTO sync_file_dirty VALUES(?1,(SELECT revision FROM sync_file_revision)) ON CONFLICT DO NOTHING",[id])?;
    }
    if projected || model_changed {
        store::save(&tx, revision, &model)?;
    }
    for folder in &scan.folders {
        let category = category_ids
            .get(&folder.to_ascii_lowercase())
            .cloned()
            .or_else(|| directories.get(folder).cloned().flatten());
        if directories.get(folder) != Some(&category) {
            tx.execute("INSERT INTO sync_directory_cache VALUES(?1,?2,?3) ON CONFLICT(library_id,path) DO UPDATE SET category_id=excluded.category_id",params![target,folder,category])?;
        }
    }
    for folder in directories.keys().filter(|f| !scan.folders.contains(*f)) {
        tx.execute(
            "DELETE FROM sync_directory_cache WHERE library_id=?1 AND path=?2",
            params![target, folder],
        )?;
    }
    for entry in scan.current.values_mut() {
        if !scan.moving.contains(&entry.file.key)
            && let Some(id) = hashes.get(&entry.object.hash)
        {
            entry.id = id.clone();
            entry.managed = chosen.get(&entry.object.hash) == Some(&entry.file.key);
        }
        if scan.old.get(&entry.file.key).is_none_or(|old| {
            old.file != entry.file
                || old.id != entry.id
                || old.managed != entry.managed
                || old.object != entry.object
        }) {
            save_entry(&tx, target, entry)?;
        }
    }
    for path in scan.old.keys().filter(|p| !scan.current.contains_key(*p)) {
        tx.execute(
            "DELETE FROM sync_path_cache WHERE library_id=?1 AND path=?2",
            params![target, path],
        )?;
    }
    tx.commit()?;
    Ok(changed)
}

#[derive(Clone)]
struct Move {
    id: String,
    source: String,
    destination: String,
    object: Object,
    revision: i64,
}

type Plan = (Vec<Move>, Vec<(String, String)>);

pub(super) fn push_files(
    library: &Mutex<Library>,
    storage: &dyn Storage,
    target: &str,
    root: &Path,
    scanned: &BTreeMap<String, FileEntry>,
    progress: &dyn Fn(SyncProgress),
) -> Result<()> {
    let cache = cached(library, target)?;
    // 先用本轮扫描时的列表试算：没有要上传、移动的文件，也没有要新建的文件夹，就不再列举远端。
    // 路径缓存刚按这份列表核对过，空闲时结果和重新列举一样。有活要干时撤销试算，
    // 重新列举后再算，上传前尽量用最新的远端状态。
    let (moves, new_folders) = plan(library, storage, target, &cache, scanned, true)?;
    if moves.is_empty() && new_folders.is_empty() {
        return Ok(());
    }
    let listed = storage
        .list("")?
        .into_iter()
        .map(|f| (f.key.clone(), f))
        .collect::<BTreeMap<_, _>>();
    let (moves, new_folders) = plan(library, storage, target, &cache, &listed, false)?;
    for (_, name) in &new_folders {
        storage.write(&format!("{name}/"), b"")?;
    }
    let total = moves.len();
    let mut sent = Vec::new();
    let mut first_error = None;
    for (index, task) in moves.into_iter().enumerate() {
        progress(SyncProgress {
            stage: TransferStage::Uploading,
            completed: index,
            total: Some(total),
        });
        let same = cache.get(&task.destination).is_some_and(|e| {
            e.object.hash == task.object.hash && listed.get(&task.destination) == Some(&e.file)
        });
        if !same {
            let bytes = match fs::read(root.join(task.object.key())) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("跳过无法读取的本机原图 {}：{e}", task.id);
                    continue;
                }
            };
            if format!("{:x}", Sha256::digest(&bytes)) != task.object.hash {
                eprintln!("跳过校验失败的本机原图 {}", task.id);
                continue;
            }
            if let Err(e) = storage.write(&task.destination, &bytes) {
                first_error = Some(e);
                break;
            }
        }
        sent.push(task);
        progress(SyncProgress {
            stage: TransferStage::Uploading,
            completed: index + 1,
            total: Some(total),
        });
    }
    let after = if sent.is_empty() {
        listed.clone()
    } else {
        storage
            .list("")?
            .into_iter()
            .map(|f| (f.key.clone(), f))
            .collect::<BTreeMap<_, _>>()
    };
    let mut completed = BTreeSet::new();
    for task in &sent {
        if !after
            .get(&task.destination)
            .is_some_and(|f| f.size == task.object.byte_size as u64)
        {
            continue;
        }
        if !task.source.is_empty() && task.source != task.destination {
            // 列举后路径若被另一张图覆盖，保留它，不能按旧移动意图删除新内容。
            let ours = cache.get(&task.source).is_some_and(|e| {
                e.object.hash == task.object.hash && after.get(&task.source) == Some(&e.file)
            });
            if ours && let Err(e) = storage.delete(&task.source) {
                if first_error.is_none() {
                    first_error = Some(e);
                }
                continue;
            }
        }
        completed.insert(task.id.clone());
    }
    if !sent.is_empty() || !new_folders.is_empty() {
        let mut lib = lock(library)?;
        let device = lib.device.id.clone();
        let tx = lib.conn.transaction()?;
        let (mut revision, mut model) = store::load(&tx)?;
        store::flush(&tx, &mut model, &mut revision)?;
        for task in sent {
            if model.dead(Kind::Sticker, &task.id) {
                continue;
            }
            let version = after
                .get(&task.destination)
                .filter(|f| f.size == task.object.byte_size as u64)
                .and_then(|f| f.version.clone());
            let previous = model.file_history.get(&task.id).and_then(|h| {
                if trash(&task.destination) {
                    h.trash.as_ref()
                } else {
                    h.live.as_ref()
                }
            });
            if previous.is_none_or(|l| {
                l.path != task.destination
                    || l.data.hash != task.object.hash
                    || l.version != version
            }) {
                append_apply(
                    &tx,
                    &mut model,
                    &device,
                    Action::File {
                        id: task.id.clone(),
                        path: task.destination.clone(),
                        data: task.object.clone(),
                        version,
                    },
                )?;
            }
            if completed.contains(&task.id) {
                if cache.get(&task.source).is_some_and(|e| {
                    e.object.hash == task.object.hash && after.get(&task.source) == Some(&e.file)
                }) || !after.contains_key(&task.source)
                {
                    tx.execute(
                        "DELETE FROM sync_path_cache WHERE library_id=?1 AND path=?2",
                        params![target, task.source],
                    )?;
                }
                save_entry(
                    &tx,
                    target,
                    &Entry {
                        file: after[&task.destination].clone(),
                        id: task.id.clone(),
                        object: task.object,
                        managed: true,
                    },
                )?;
                tx.execute(
                    "DELETE FROM sync_file_moves WHERE library_id=?1 AND sticker_id=?2",
                    params![target, task.id],
                )?;
                tx.execute(
                    "DELETE FROM sync_file_dirty WHERE sticker_id=?1 AND revision=?2",
                    params![task.id, task.revision],
                )?;
            }
        }
        for (id, name) in new_folders {
            tx.execute("INSERT INTO sync_directory_cache VALUES(?1,?2,?3) ON CONFLICT(library_id,path) DO UPDATE SET category_id=excluded.category_id",params![target,name,id])?;
        }
        store::save(&tx, revision, &model)?;
        tx.commit()?;
    }
    if let Some(e) = first_error {
        return Err(e);
    }
    Ok(())
}

/// 算出要上传或移动的文件和要新建的分类文件夹，并记入 `sync_file_moves`。
/// `trial` 为真且有活要干时撤销整个事务，只把结果交给调用方判断。
fn plan(
    library: &Mutex<Library>,
    storage: &dyn Storage,
    target: &str,
    cache: &BTreeMap<String, Entry>,
    listed: &BTreeMap<String, FileEntry>,
    trial: bool,
) -> Result<Plan> {
    let chosen = winners(cache);
    let mut used = BTreeMap::<String, BTreeSet<String>>::new();
    let mut folders = BTreeMap::<String, String>::new();
    for path in listed.keys() {
        used.entry(path.to_lowercase()).or_default().insert(
            cache
                .get(path)
                .map(|e| e.object.hash.clone())
                .unwrap_or_default(),
        );
        if let Some((Some(c), _, _)) = paths::image(path) {
            folders
                .entry(c.to_ascii_lowercase())
                .or_insert_with(|| c.into());
        }
    }
    let mut lib = lock(library)?;
    let device = lib.device.id.clone();
    let tx = lib.conn.transaction()?;
    let (mut revision, mut model) = store::load(&tx)?;
    let before = revision;
    store::flush(&tx, &mut model, &mut revision)?;
    if revision != before {
        store::save(&tx, revision, &model)?;
    }
    let categories = model.projected(Kind::Category);
    let new_folders = categories
        .iter()
        .filter_map(|(id, e)| {
            e.fields
                .get("name")
                .filter(|r| r.stamp.device == device)
                .and_then(|r| r.value.as_str())
                .filter(|name| {
                    paths::component(name)
                        && !listed.keys().any(|p| p.starts_with(&format!("{name}/")))
                })
                .map(|name| (id.clone(), name.to_owned()))
        })
        .collect::<Vec<_>>();
    for (_, name) in &new_folders {
        storage.validate_write(&format!("{name}/"))?;
    }
    let dirty = {
        let mut q = tx.prepare("SELECT sticker_id,revision FROM sync_file_dirty")?;
        q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()?
    };
    let local = {
        let mut q=tx.prepare("SELECT s.id,s.name,c.name,s.deleted_at FROM stickers s LEFT JOIN categories c ON c.id=s.category_id")?;
        q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ),
            ))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?
    };
    let pending = {
        let mut q=tx.prepare("SELECT sticker_id,source,destination,object,revision FROM sync_file_moves WHERE library_id=?1")?;
        q.query_map([target], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut moves = BTreeMap::new();
    for (id, source, destination, json, rev) in pending {
        let object: Object = serde_json::from_str(&json)?;
        let overwritten = cache
            .get(&destination)
            .is_some_and(|e| e.object.hash != object.hash)
            || cache
                .get(&source)
                .is_some_and(|e| e.object.hash != object.hash);
        let renamed_incompatible = dirty.get(&id).is_some_and(|current| *current > rev)
            && storage.validate_write(&destination).is_err();
        if model.dead(Kind::Sticker, &id) || overwritten || renamed_incompatible {
            tx.execute(
                "DELETE FROM sync_file_moves WHERE library_id=?1 AND sticker_id=?2",
                params![target, id],
            )?;
        } else {
            used.entry(destination.to_lowercase())
                .or_default()
                .insert(object.hash.clone());
            moves.insert(
                id.clone(),
                Move {
                    id,
                    source,
                    destination,
                    object,
                    revision: rev,
                },
            );
        }
    }
    for (id, e) in model.projected(Kind::Sticker) {
        if moves.contains_key(&id) {
            continue;
        }
        let Some(object) = e.data else { continue };
        let Some((name, category, deleted)) = local.get(&id) else {
            continue;
        };
        let source = chosen.get(&object.hash).cloned().unwrap_or_default();
        let destination = if deleted.is_some() {
            format!(".sticker-manager/trash/{id}.{}", object.ext)
        } else if !dirty.contains_key(&id) && !source.is_empty() && !trash(&source) {
            source.clone()
        } else {
            let category = category.as_deref().map(|c| {
                folders
                    .get(&c.to_ascii_lowercase())
                    .map(String::as_str)
                    .unwrap_or(c)
            });
            let mut n = 1;
            let path = loop {
                let path = match paths::key(category, name, &object.ext, n) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("跳过表情 {id} 的文件路径：{e}");
                        break None;
                    }
                };
                if used
                    .get(&path.to_lowercase())
                    .is_none_or(|hs| hs.len() == 1 && hs.contains(&object.hash))
                    || path == source
                {
                    break Some(path);
                }
                n += 1;
            };
            let Some(path) = path else { continue };
            path
        };
        if source == destination
            && cache
                .get(&source)
                .is_some_and(|e| listed.get(&source).is_some_and(|f| *f == e.file))
        {
            tx.execute("DELETE FROM sync_file_dirty WHERE sticker_id=?1", [&id])?;
            continue;
        }
        storage.validate_write(&destination)?;
        let revision = dirty.get(&id).copied().unwrap_or(-1);
        used.entry(destination.to_lowercase())
            .or_default()
            .insert(object.hash.clone());
        tx.execute(
            "INSERT INTO sync_file_moves VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                target,
                id,
                source,
                destination,
                serde_json::to_string(&object)?,
                revision
            ],
        )?;
        moves.insert(
            id.clone(),
            Move {
                id,
                source,
                destination,
                object,
                revision,
            },
        );
    }
    let plan = (moves.into_values().collect::<Vec<_>>(), new_folders);
    if trial && (!plan.0.is_empty() || !plan.1.is_empty()) {
        return Ok(plan);
    }
    tx.commit()?;
    Ok(plan)
}

pub(super) fn purge(library: &Mutex<Library>, storage: &dyn Storage, target: &str) -> Result<()> {
    let cache = cached(library, target)?;
    let (_, model) = store::load(&lock(library)?.conn)?;
    let live = model
        .projected(Kind::Sticker)
        .into_values()
        .filter_map(|e| e.data.map(|o| o.hash))
        .collect::<BTreeSet<_>>();
    for (path, entry) in cache {
        if entry.managed
            && model.dead(Kind::Sticker, &entry.id)
            && !live.contains(&entry.object.hash)
        {
            storage.delete(&path)?;
            lock(library)?.conn.execute(
                "DELETE FROM sync_path_cache WHERE library_id=?1 AND path=?2",
                params![target, path],
            )?;
        }
    }
    let directories = {
        let lib = lock(library)?;
        let mut q = lib
            .conn
            .prepare("SELECT path,category_id FROM sync_directory_cache WHERE library_id=?1")?;
        q.query_map([target], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let categories = model.projected(Kind::Category);
    let candidates = directories
        .into_iter()
        .filter(|(folder, id)| {
            id.as_ref().is_some_and(|id| {
                let renamed = categories
                    .get(&model.resolve(Kind::Category, id))
                    .and_then(|e| e.fields.get("name"))
                    .and_then(|r| r.value.as_str())
                    .is_some_and(|name| !name.eq_ignore_ascii_case(folder));
                model.dead(Kind::Category, id) || renamed
            })
        })
        .map(|(folder, _)| folder)
        .collect::<Vec<_>>();
    // 绝大多数轮次没有已删除或已改名分类的旧文件夹，不必为确认它们是否已空列举远端。
    if candidates.is_empty() {
        return Ok(());
    }
    let listed = storage.list("")?;
    for folder in candidates {
        let prefix = format!("{folder}/");
        if !listed
            .iter()
            .any(|f| f.key.starts_with(&prefix) && f.key != prefix)
        {
            storage.delete(&prefix)?;
            lock(library)?.conn.execute(
                "DELETE FROM sync_directory_cache WHERE library_id=?1 AND path=?2",
                params![target, folder],
            )?;
        }
    }
    Ok(())
}
