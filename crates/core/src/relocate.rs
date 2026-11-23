use crate::{Error, Library, Result};
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::NamedTempFile;
pub const MANAGED: [&str; 6] = [
    "library.db",
    "library.db-wal",
    "library.db-shm",
    "objects",
    "thumbs",
    "local",
];
pub struct TargetRules<'a> {
    pub current_root: &'a Path,
    pub default_root: &'a Path,
    pub pointer_name: &'a str,
    pub sync_folder: Option<&'a Path>,
    pub forbidden: &'a [(PathBuf, &'static str)],
}
#[derive(Default, Serialize)]
pub struct ContentSize {
    pub files: usize,
    pub bytes: u64,
}
#[derive(Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocateProgress {
    pub copied: usize,
    pub total: usize,
    pub bytes: u64,
    pub total_bytes: u64,
}
pub enum CopyOutcome {
    Completed,
    Cancelled,
}
fn fail(s: &str) -> Error {
    Error::LibraryLocation(s.into())
}
pub fn check_relocation_target(target: &Path, r: &TargetRules) -> Result<PathBuf> {
    if !target.is_absolute() {
        return Err(fail("请选择一个完整的文件夹路径"));
    }
    if !target.is_dir() {
        return Err(fail("找不到这个文件夹"));
    }
    let t = target.canonicalize()?;
    let cur = r.current_root.canonicalize()?;
    let base = r.default_root.canonicalize()?;
    let overlaps = |p: &Path| t.starts_with(p) || p.starts_with(&t);
    if t == cur {
        return Err(fail("仓库已经在这个位置"));
    }
    if overlaps(&cur) {
        return Err(fail("新位置不能在当前仓库里面，也不能包含当前仓库"));
    }
    if t != base && overlaps(&base) {
        return Err(fail("新位置不能在默认仓库文件夹里面，也不能包含它"));
    }
    if r.sync_folder
        .is_some_and(|p| overlaps(&p.canonicalize().unwrap_or_else(|_| p.into())))
    {
        return Err(fail("新位置与同步文件夹重叠"));
    }
    for (p, msg) in r.forbidden {
        if t.starts_with(p.canonicalize().unwrap_or_else(|_| p.clone())) {
            return Err(fail(msg));
        }
    }
    let mut unexpected = Vec::new();
    for entry in fs::read_dir(&t)? {
        let name = entry?.file_name();
        let n = name.to_string_lossy();
        if n.starts_with('.')
            || n.eq_ignore_ascii_case("desktop.ini")
            || n.eq_ignore_ascii_case("thumbs.db")
            || n == "Icon\r"
            || (t == base && n == r.pointer_name)
        {
            continue;
        }
        unexpected.push(n.into_owned());
        if unexpected.len() == 3 {
            break;
        }
    }
    if !unexpected.is_empty() {
        return Err(fail(&if t == base {
            format!(
                "默认位置里还有其他文件（{}），请先移走后再恢复默认位置",
                unexpected.join("、")
            )
        } else {
            "请选择一个空文件夹".into()
        }));
    }
    let probe = || -> io::Result<()> {
        let mut probe = NamedTempFile::new_in(&t)?;
        probe.write_all(&[0])?;
        probe.as_file().sync_all()
    };
    probe().map_err(|e| fail(&format!("这个文件夹不能写入：{e}")))?;
    Ok(t)
}
fn files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for e in fs::read_dir(dir)? {
        let e = e?;
        if !e.file_name().to_string_lossy().starts_with(".tmp") && e.file_type()?.is_file() {
            files.push(e.path())
        }
    }
    Ok(files)
}
pub fn content_size(root: &Path) -> Result<ContentSize> {
    let mut paths = vec![root.join("library.db")];
    for d in ["local", "objects", "thumbs"] {
        paths.extend(files(&root.join(d))?)
    }
    let mut size = ContentSize::default();
    for p in paths {
        if let Some(m) = p.metadata().map(Some).or_else(ignore_missing)? {
            size.files += 1;
            size.bytes += m.len()
        }
    }
    Ok(size)
}
fn ignore_missing<T>(e: io::Error) -> io::Result<Option<T>> {
    if e.kind() == io::ErrorKind::NotFound {
        Ok(None)
    } else {
        Err(e)
    }
}
fn sync_dir(p: &Path) -> io::Result<()> {
    #[cfg(unix)]
    File::open(p)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = p;
    Ok(())
}
fn copy_file(src: &Path, dst: &Path) -> Result<u64> {
    let n = fs::copy(src, dst).map_err(|e| {
        if e.kind() == io::ErrorKind::StorageFull {
            fail("目标磁盘空间不足")
        } else {
            e.into()
        }
    })?;
    let f = File::options().write(true).open(dst)?;
    if f.metadata()?.len() != n {
        return Err(fail("文件复制不完整"));
    }
    f.sync_all()?;
    Ok(n)
}
pub fn copy_content(
    source: &Path,
    target: &Path,
    cancel: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(RelocateProgress),
) -> Result<CopyOutcome> {
    let mut list = Vec::new();
    let mut p = RelocateProgress::default();
    for d in ["objects", "thumbs"] {
        let dir = target.join(d);
        fs::create_dir_all(&dir)?;
        for src in files(&source.join(d))? {
            if let Some(m) = src.metadata().map(Some).or_else(ignore_missing)? {
                p.total_bytes += m.len();
                list.push((dir.join(src.file_name().unwrap()), src))
            }
        }
    }
    p.total = list.len();
    progress(p);
    for (dst, src) in list {
        if cancel() {
            return Ok(CopyOutcome::Cancelled);
        }
        match copy_file(&src, &dst) {
            Ok(n) => p.bytes += n,
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound && !src.try_exists()? => (),
            Err(e) => return Err(e),
        }
        p.copied += 1;
        progress(p)
    }
    for d in ["objects", "thumbs"] {
        sync_dir(&target.join(d))?;
    }
    Ok(if cancel() {
        CopyOutcome::Cancelled
    } else {
        CopyOutcome::Completed
    })
}
pub fn display_path(p: &Path) -> PathBuf {
    #[cfg(windows)]
    if let Some(s) = p.to_str() {
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}").into();
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return rest.into();
        }
    }
    p.into()
}
impl Library {
    pub fn relocate(
        &mut self,
        target: &Path,
        commit: &mut dyn FnMut(&Path) -> Result<()>,
    ) -> Result<PathBuf> {
        let db = target.join("library.db");
        File::create_new(&db)?;
        let path = display_path(&db);
        self.conn.execute(
            "VACUUM INTO ?1",
            [path.to_str().ok_or_else(|| fail("仓库路径必须是 UTF-8"))?],
        )?;
        File::options().write(true).open(db)?.sync_all()?;
        let local = target.join("local");
        fs::create_dir(&local)?;
        for src in files(&self.root.join("local"))? {
            let mut out = NamedTempFile::new_in(&local)?;
            io::copy(&mut File::open(&src)?, &mut out)?;
            out.as_file().sync_all()?;
            out.persist_noclobber(local.join(src.file_name().unwrap()))
                .map_err(|e| e.error)?;
        }
        sync_dir(&local)?;
        for d in ["objects", "thumbs"] {
            let dir = target.join(d);
            fs::create_dir_all(&dir)?;
            for src in files(&self.root.join(d))? {
                let dst = dir.join(src.file_name().unwrap());
                let size = src.metadata()?.len();
                if dst
                    .metadata()
                    .map(Some)
                    .or_else(ignore_missing)?
                    .is_none_or(|m| m.len() != size)
                {
                    copy_file(&src, &dst)?;
                }
                if dst.metadata()?.len() != size {
                    return Err(fail("文件大小不一致"));
                }
            }
            sync_dir(&dir)?
        }
        let next = Library::open_existing(target)?;
        let count = |lib: &Library| {
            lib.conn
                .query_row("SELECT COUNT(*) FROM stickers", [], |r| r.get::<_, i64>(0))
        };
        if next.device.id != self.device.id || count(&next)? != count(self)? {
            return Err(fail("设备身份或表情数量不一致"));
        }
        sync_dir(target)?;
        commit(next.root())?;
        let old = std::mem::replace(self, next);
        Ok(old.root.clone())
    }
}
pub fn remove_content(root: &Path) -> Vec<PathBuf> {
    let mut left = Vec::new();
    for entry in MANAGED {
        let path = root.join(entry);
        let remove = || {
            if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            }
            .map(Some)
            .or_else(ignore_missing)
        };
        if remove().is_err() {
            std::thread::sleep(Duration::from_millis(300));
            if remove().is_err() {
                left.push(path)
            }
        }
    }
    left
}

#[cfg(test)]
mod tests;
