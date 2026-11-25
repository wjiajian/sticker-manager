use crate::{
    Error, Library, Result,
    import::archive::{ARCHIVE_LIMIT, ENTRY_COUNT_LIMIT, ENTRY_LIMIT},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, UNIX_EPOCH},
};
use tempfile::Builder;
use zip::{ZipWriter, write::SimpleFileOptions};
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportScope {
    Library,
    Category(String),
    Stickers(Vec<String>),
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Folder,
    Zip,
}
pub struct ExportRequest {
    pub scope: ExportScope,
    pub destination: PathBuf,
    pub name: String,
    pub format: ExportFormat,
    pub forbidden: Vec<PathBuf>,
    pub utc_offset_minutes: i32,
}
#[derive(Clone, Copy, Default, Serialize)]
pub struct ExportProgress {
    pub processed: usize,
    pub total: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFailure {
    pub sticker_id: String,
    pub name: String,
    pub message: String,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub exported: u32,
    pub skipped: u32,
    pub bytes: u64,
    pub cancelled: bool,
    pub failures: Vec<ExportFailure>,
    pub output: Option<PathBuf>,
    pub exceeds_import_limits: bool,
}

pub fn sanitize_component(raw: &str, fallback: &str) -> String {
    let mut s: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    while s.contains("..") {
        s = s.replace("..", ".");
    }
    let trim = |c: char| c == '.' || c.is_whitespace();
    let mut s = s.trim_matches(trim).to_owned();
    s.truncate(s.floor_char_boundary(120));
    let s = s.trim_end_matches(trim);
    if s.is_empty() {
        return fallback.into();
    }
    let base = s
        .split('.')
        .next()
        .unwrap_or(s)
        .trim_end()
        .to_ascii_uppercase();
    let reserved = matches!(&*base, "CON" | "PRN" | "AUX" | "NUL")
        || base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"))
            .is_some_and(|n| {
                matches!(
                    n,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            });
    if reserved { format!("_{s}") } else { s.into() }
}
#[derive(Default)]
struct NameSet(HashSet<String>);
impl NameSet {
    fn claim(&mut self, stem: &str, ext: &str) -> String {
        for i in 1.. {
            let suffix = if i == 1 {
                String::new()
            } else {
                format!(" ({i})")
            };
            let name = format!(
                "{}{suffix}{ext}",
                &stem[..stem.floor_char_boundary(120 - suffix.len())]
            );
            if self.0.insert(name.to_lowercase()) {
                return name;
            }
        }
        unreachable!()
    }
}
struct Item {
    id: String,
    name: String,
    source: PathBuf,
    folder: String,
    ext: String,
    added: i64,
}
struct Snapshot {
    root: PathBuf,
    items: Vec<Item>,
    skipped: u32,
}
impl Library {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn export(&self, r: &ExportRequest) -> Result<ExportReport> {
        write(snapshot(self, &r.scope)?, r, &|| false, &mut |_| {})
    }
}
fn snapshot(lib: &Library, scope: &ExportScope) -> Result<Snapshot> {
    let (filter, param, requested) = match scope {
        ExportScope::Library => ("1", String::new(), 0),
        ExportScope::Category(id) => {
            if !lib.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM categories WHERE id=?1)",
                [id],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(Error::CategoryUnavailable);
            }
            ("s.category_id=?1", id.clone(), 0)
        }
        ExportScope::Stickers(ids) => (
            "s.id IN (SELECT value FROM json_each(?1))",
            serde_json::to_string(ids)?,
            ids.iter().collect::<HashSet<_>>().len(),
        ),
    };
    let mut stmt = lib.conn.prepare(&format!(
        "SELECT s.id, s.name, s.hash, s.ext, s.added_at, c.id, c.name
         FROM stickers s LEFT JOIN categories c ON c.id = s.category_id
         WHERE s.deleted_at IS NULL AND {filter}
         ORDER BY c.id IS NULL, c.position, c.id, s.added_at, s.id"
    ))?;
    let mut rows = stmt.query(rusqlite::params_from_iter(
        (stmt.parameter_count() > 0).then_some(param),
    ))?;
    let mut dirs = NameSet::default();
    dirs.0.insert("未分类".into());
    let mut categories = HashMap::new();
    let mut items = Vec::new();
    while let Some(r) = rows.next()? {
        let hash: String = r.get(2)?;
        let ext: String = r.get(3)?;
        if !matches!(&*ext, "jpg" | "png" | "gif" | "webp")
            || hash.len() != 64
            || !hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::Export("仓库图片路径无效".into()));
        }
        let folder = if let Some(id) = r.get::<_, Option<String>>(5)? {
            let name: String = r.get(6)?;
            categories
                .entry(id)
                .or_insert_with(|| dirs.claim(&sanitize_component(&name, "分类"), ""))
                .clone()
        } else {
            "未分类".into()
        };
        items.push(Item {
            id: r.get(0)?,
            name: r.get(1)?,
            source: lib.root.join("objects").join(format!("{hash}.{ext}")),
            folder,
            ext: format!(".{ext}"),
            added: r.get(4)?,
        })
    }
    if items.is_empty() {
        return Err(Error::Export("没有可导出的表情".into()));
    }
    Ok(Snapshot {
        root: lib.root.clone(),
        skipped: requested.saturating_sub(items.len()) as u32,
        items,
    })
}
pub fn export_with_progress(
    lib: &Mutex<Library>,
    r: &ExportRequest,
    cancel: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(ExportProgress),
) -> Result<ExportReport> {
    let snapshot = {
        let guard = lib.lock().map_err(|_| Error::LibraryUnavailable)?;
        snapshot(&guard, &r.scope)?
    };
    write(snapshot, r, cancel, progress)
}
fn exceeds_limits(count: u32, total: u64, size: u64) -> bool {
    count as usize > ENTRY_COUNT_LIMIT || total > ARCHIVE_LIMIT || size > ENTRY_LIMIT
}
fn zip_time(ms: i64, offset: i32) -> zip::DateTime {
    let Ok(t) =
        time::OffsetDateTime::from_unix_timestamp(ms.div_euclid(1000) + i64::from(offset) * 60)
    else {
        return zip::DateTime::default();
    };
    zip::DateTime::from_date_and_time(
        t.year() as u16,
        t.month() as u8,
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
    )
    .unwrap_or_default()
}
fn output_error(e: io::Error) -> Error {
    Error::Export(if e.kind() == io::ErrorKind::StorageFull {
        "导出位置空间不足".into()
    } else {
        format!("写入导出位置失败：{e}")
    })
}
// A source read failure can be isolated to one sticker. A destination failure aborts the batch.
fn transfer(input: &mut File, out: &mut impl Write) -> Result<io::Result<u64>> {
    let mut buf = [0; 65536];
    let mut bytes = 0;
    loop {
        match input.read(&mut buf) {
            Ok(0) => return Ok(Ok(bytes)),
            Ok(n) => {
                out.write_all(&buf[..n]).map_err(output_error)?;
                bytes += n as u64
            }
            Err(e) => return Ok(Err(e)),
        }
    }
}
fn claim_directory(parent: &Path, stem: &str) -> Result<PathBuf> {
    let mut names = NameSet::default();
    loop {
        let path = parent.join(names.claim(stem, ""));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(output_error(e)),
        }
    }
}

struct FolderOutput {
    root: PathBuf,
    flat: bool,
    folders: HashMap<String, PathBuf>,
}
impl FolderOutput {
    fn append(
        &mut self,
        item: &Item,
        input: &mut File,
        names: &mut NameSet,
    ) -> Result<io::Result<u64>> {
        let dir = if self.flat {
            &self.root
        } else {
            match self.folders.entry(item.folder.clone()) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(claim_directory(&self.root, &item.folder)?)
                }
            }
        };
        let stem = sanitize_component(&item.name, "表情");
        for _ in 0..1000 {
            let path = dir.join(names.claim(&stem, &item.ext));
            let mut out = match File::create_new(&path) {
                Ok(f) => f,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(output_error(e)),
            };
            let result = transfer(input, &mut out)?;
            if result.is_err() {
                drop(out);
                fs::remove_file(path)?;
            } else if let Some(t) =
                UNIX_EPOCH.checked_add(Duration::from_millis(item.added.max(0) as u64))
            {
                let _ = out.set_modified(t);
            }
            return Ok(result);
        }
        Err(Error::Export("同名文件过多".into()))
    }
}

struct ZipOutput {
    archive: ZipWriter<BufWriter<File>>,
    temporary: tempfile::NamedTempFile,
}
impl ZipOutput {
    fn new(parent: &Path) -> Result<Self> {
        let temporary = Builder::new()
            .prefix(".sticker-export-")
            .suffix(".tmp")
            .tempfile_in(parent)?;
        let archive = ZipWriter::new(BufWriter::with_capacity(1 << 20, temporary.reopen()?));
        Ok(Self { archive, temporary })
    }
    fn append(
        &mut self,
        item: &Item,
        input: &mut File,
        names: &mut NameSet,
        offset: i32,
    ) -> Result<io::Result<u64>> {
        let size = match input.metadata() {
            Ok(m) => m.len(),
            Err(e) => return Ok(Err(e)),
        };
        let name = names.claim(&sanitize_component(&item.name, "表情"), &item.ext);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .large_file(size >= u32::MAX as u64)
            .last_modified_time(zip_time(item.added, offset));
        self.archive
            .start_file(format!("{}/{name}", item.folder), options)
            .map_err(|e| output_error(e.into()))?;
        let result = transfer(input, &mut self.archive)?;
        if result.is_err() {
            self.archive
                .abort_file()
                .map_err(|e| output_error(e.into()))?;
        }
        Ok(result)
    }
    fn finish(self, dest: &Path) -> Result<PathBuf> {
        self.archive
            .finish()
            .map_err(|e| output_error(e.into()))?
            .into_inner()
            .map_err(|e| output_error(e.into_error()))?
            .sync_all()?;
        if dest
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        {
            self.temporary
                .persist(dest)
                .map_err(|e| output_error(e.error))?;
            return Ok(dest.into());
        }
        let mut path = dest.as_os_str().to_owned();
        path.push(".zip");
        self.temporary.persist_noclobber(&path).map_err(|e| {
            if e.error.kind() == io::ErrorKind::AlreadyExists {
                Error::Export("同名文件已存在".into())
            } else {
                output_error(e.error)
            }
        })?;
        Ok(path.into())
    }
}

enum Output {
    Folder(FolderOutput),
    Zip(Box<ZipOutput>),
}
fn write(
    s: Snapshot,
    r: &ExportRequest,
    cancel: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(ExportProgress),
) -> Result<ExportReport> {
    let zip = r.format == ExportFormat::Zip;
    let dest = &r.destination;
    if !dest.is_absolute() {
        return Err(Error::Export("请选择完整的导出路径".into()));
    }
    let parent = if zip {
        dest.parent()
            .ok_or_else(|| Error::Export("请选择压缩包路径".into()))?
    } else {
        dest
    }
    .canonicalize()?;
    if !parent.is_dir() {
        return Err(Error::Export("找不到导出文件夹".into()));
    }
    if parent.starts_with(&s.root) {
        return Err(Error::Export("不能导出到仓库文件夹里".into()));
    }
    if r.forbidden
        .iter()
        .filter_map(|p| p.canonicalize().ok())
        .any(|p| parent.starts_with(p))
    {
        return Err(Error::Export("不能导出到同步文件夹里".into()));
    }
    let category = matches!(r.scope, ExportScope::Category(_));
    let stem = if category {
        s.items[0].folder.clone()
    } else {
        sanitize_component(&r.name, "表情导出")
    };
    let mut output = if zip {
        Output::Zip(Box::new(ZipOutput::new(&parent)?))
    } else {
        Output::Folder(FolderOutput {
            root: claim_directory(&parent, &stem)?,
            flat: category,
            folders: HashMap::new(),
        })
    };
    let mut report = ExportReport {
        skipped: s.skipped,
        ..Default::default()
    };
    let mut names: HashMap<String, NameSet> = HashMap::new();
    let total = s.items.len();
    let result = (|| -> Result<()> {
        for (index, item) in s.items.into_iter().enumerate() {
            if cancel() {
                report.cancelled = true;
                break;
            }
            let names = names.entry(item.folder.clone()).or_default();
            let copy = match File::open(&item.source) {
                Err(e) => Err(e),
                Ok(mut file) => match &mut output {
                    Output::Folder(folder) => folder.append(&item, &mut file, names)?,
                    Output::Zip(zip) => {
                        zip.append(&item, &mut file, names, r.utc_offset_minutes)?
                    }
                },
            };
            match copy {
                Ok(n) => {
                    report.exported += 1;
                    report.bytes += n;
                    report.exceeds_import_limits |=
                        zip && exceeds_limits(report.exported, report.bytes, n)
                }
                Err(e) => report.failures.push(ExportFailure {
                    sticker_id: item.id,
                    name: item.name,
                    message: if e.kind() == io::ErrorKind::NotFound {
                        "找不到原图文件".into()
                    } else {
                        e.to_string()
                    },
                }),
            }
            progress(ExportProgress {
                processed: index + 1,
                total,
            });
        }
        report.cancelled |= cancel();
        Ok(())
    })();
    let keep = result.is_ok() && !report.cancelled && report.exported > 0;
    match output {
        Output::Folder(folder) if keep => {
            report.output = Some(crate::relocate::display_path(&folder.root))
        }
        Output::Folder(folder) if result.is_err() => {
            let _ = fs::remove_dir_all(folder.root);
        }
        Output::Folder(folder) => fs::remove_dir_all(folder.root)?,
        Output::Zip(zip) if keep => report.output = Some(zip.finish(dest)?),
        Output::Zip(_) => (),
    }
    result?;
    Ok(report)
}

#[cfg(test)]
mod tests;
