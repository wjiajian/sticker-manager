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
}

/// 后端只中转文件。write 必须原子发布，NotFound 通过 Ok(None) 表达。
pub trait Storage: Send + Sync {
    fn list(&self, prefix: &str) -> Result<Vec<FileEntry>>;
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>>;
    fn write(&self, key: &str, bytes: &[u8]) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
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
        if key.is_empty()
            || key.contains('\\')
            || Path::new(key)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(Error::Sync("无效的存储文件名".into()));
        }
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
        match fs::remove_file(self.path(key)?) {
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
        Ok(self
            .inner
            .lock()
            .unwrap()
            .visible
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(key, bytes)| FileEntry {
                key: key.clone(),
                size: bytes.len() as u64,
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
