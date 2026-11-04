use crate::{Error, Result};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub key: String,
    pub size: u64,
    pub version: Option<String>,
}

/// 后端只中转文件。写入可被读到一部分，读取方须校验完整性后再应用。
/// read 的 NotFound 通过 Ok(None) 表达；delete 不存在的文件应成功。
pub trait Storage: Send + Sync {
    fn list(&self, prefix: &str) -> Result<Vec<FileEntry>>;
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>>;
    fn write(&self, key: &str, bytes: &[u8]) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

pub fn validate_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.contains('\\')
        || key.contains("..")
        || Path::new(key)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::Sync("无效的存储文件名".into()));
    }
    let visible = key.strip_prefix(super::paths::META).unwrap_or(key);
    if visible.split('/').any(|p| p.starts_with('.')) && !key.starts_with(".sticker-probe-") {
        return Err(Error::Sync("无效的存储文件名".into()));
    }
    Ok(())
}

// 仅限制实际写入 Windows 文件系统的路径；网络存储继续使用原有同步名称。
#[cfg(target_os = "windows")]
fn validate_windows_file_name(key: &str) -> Result<()> {
    for name in key.trim_end_matches('/').split('/') {
        let base = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
        let base = base.to_ascii_uppercase();
        let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || base
                .strip_prefix("COM")
                .or_else(|| base.strip_prefix("LPT"))
                .is_some_and(|suffix| {
                    matches!(
                        suffix,
                        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                    )
                });
        if reserved
            || name.ends_with([' ', '.'])
            || name.chars().any(|c| c < ' ' || "<>:\"\\|?*".contains(c))
            || name.encode_utf16().count() > 255
        {
            return Err(Error::Sync(format!(
                "Windows 同步文件夹不支持名称“{name}”（路径：{key}），请修改该表情或分类的名称后重试"
            )));
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct FolderStorage {
    root: PathBuf,
}
impl FolderStorage {
    pub fn new(root: &Path) -> Result<Self> {
        if !root.is_dir() {
            return Err(Error::Sync("同步文件夹不存在或不可访问".into()));
        }
        Ok(Self {
            root: root.canonicalize()?,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn path(&self, key: &str) -> Result<PathBuf> {
        validate_key(key)?;
        #[cfg(target_os = "windows")]
        validate_windows_file_name(key)?;
        let mut path = self.root.clone();
        for part in Path::new(key).components() {
            path.push(part);
            match fs::symlink_metadata(&path) {
                Ok(m) if m.file_type().is_symlink() => {
                    return Err(Error::Sync("同步文件夹内不能使用符号链接".into()));
                }
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
        }
        Ok(path)
    }
}
impl Storage for FolderStorage {
    fn list(&self, prefix: &str) -> Result<Vec<FileEntry>> {
        let mut pending = vec![self.root.clone()];
        let mut result = Vec::new();
        while let Some(path) = pending.pop() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let ty = entry.file_type()?;
                if ty.is_symlink() {
                    continue;
                }
                if ty.is_dir() {
                    let key = format!(
                        "{}/",
                        entry
                            .path()
                            .strip_prefix(&self.root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/")
                    );
                    if key.starts_with(prefix) {
                        result.push(FileEntry {
                            key,
                            size: 0,
                            version: None,
                        });
                    }
                    pending.push(entry.path());
                } else if ty.is_file() {
                    let key = entry
                        .path()
                        .strip_prefix(&self.root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    if key.starts_with(prefix) {
                        result.push(FileEntry {
                            key,
                            size: entry.metadata()?.len(),
                            version: entry
                                .metadata()?
                                .modified()
                                .ok()
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|t| t.as_nanos().to_string()),
                        });
                    }
                }
            }
        }
        result.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(result)
    }
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match fs::read(self.path(key)?) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    fn write(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path(key)?;
        if key.ends_with('/') && bytes.is_empty() {
            fs::create_dir_all(path)?;
            return Ok(());
        }
        let directory = path.parent().unwrap();
        fs::create_dir_all(directory)?;
        // 同图可能被两台设备同时上传；独立临时名避免互相改名尚未写完的文件。
        let prefix = format!(".{}.", path.file_name().unwrap().to_string_lossy());
        let mut file = tempfile::Builder::new()
            .prefix(&prefix)
            .suffix(".tmp")
            .tempfile_in(directory)?;
        file.write_all(bytes)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<()> {
        let path = self.path(key)?;
        let result = if key.ends_with('/') {
            fs::remove_dir(path)
        } else {
            fs::remove_file(path)
        };
        match result {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

/// 每个 clone 共享发送队列；hold/deliver 可模拟文件独立、乱序送达。
#[derive(Clone, Default)]
pub struct MemoryStorage {
    inner: Arc<Mutex<MemoryFiles>>,
}
#[derive(Default)]
struct MemoryFiles {
    visible: BTreeMap<String, Vec<u8>>,
    pending: BTreeMap<String, Option<Vec<u8>>>,
    hold: bool,
    clock: u64,
    versions: BTreeMap<String, String>,
}
impl MemoryStorage {
    pub fn hold(&self, hold: bool) {
        self.inner.lock().unwrap().hold = hold;
    }
    pub fn pending_keys(&self) -> Vec<String> {
        self.inner.lock().unwrap().pending.keys().cloned().collect()
    }
    pub fn deliver(&self, key: &str) {
        let mut files = self.inner.lock().unwrap();
        if let Some(value) = files.pending.remove(key) {
            match value {
                Some(bytes) => {
                    files.clock += 1;
                    let version = files.clock.to_string();
                    files.versions.insert(key.into(), version);
                    files.visible.insert(key.into(), bytes);
                }
                None => {
                    files.visible.remove(key);
                }
            }
        }
    }
    pub fn deliver_all(&self) {
        for key in self.pending_keys() {
            self.deliver(&key);
        }
    }
}
impl Storage for MemoryStorage {
    fn list(&self, prefix: &str) -> Result<Vec<FileEntry>> {
        let files = self.inner.lock().unwrap();
        Ok(files
            .visible
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(key, bytes)| FileEntry {
                key: key.clone(),
                size: bytes.len() as u64,
                version: files.versions.get(key).cloned(),
            })
            .collect())
    }
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.inner.lock().unwrap().visible.get(key).cloned())
    }
    fn write(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let mut files = self.inner.lock().unwrap();
        if files.hold {
            files.pending.insert(key.into(), Some(bytes.to_vec()));
        } else {
            files.clock += 1;
            let version = files.clock.to_string();
            files.versions.insert(key.into(), version);
            files.visible.insert(key.into(), bytes.to_vec());
        }
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<()> {
        let mut files = self.inner.lock().unwrap();
        if files.hold {
            files.pending.insert(key.into(), None);
        } else {
            files.visible.remove(key);
        }
        Ok(())
    }
}
