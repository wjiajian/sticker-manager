use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{Error, Result};

/// 只存于本机的 local/device.json，不属于同步元数据。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
}

impl Device {
    pub(crate) fn rename(&mut self, root: &Path, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::EmptyName("设备"));
        }
        let next = Self {
            id: self.id.clone(),
            name: name.into(),
        };
        let mut file = NamedTempFile::new_in(root.join("local"))?;
        file.write_all(&serde_json::to_vec(&next)?)?;
        file.as_file().sync_all()?;
        file.persist(root.join("local/device.json"))
            .map_err(|e| e.error)?;
        *self = next;
        Ok(())
    }
    pub(crate) fn load_or_create(root: &Path) -> Result<Self> {
        let directory = root.join("local");
        fs::create_dir_all(&directory)?;
        let path = directory.join("device.json");
        match fs::read(&path) {
            Ok(bytes) => return Self::parse(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let device = Self {
            id: Uuid::new_v4().to_string(),
            name: whoami::devicename().map_err(|error| Error::DeviceName(error.to_string()))?,
        };
        let bytes = serde_json::to_vec(&device).map_err(|_| Error::InvalidDevice)?;
        let mut file = NamedTempFile::new_in(directory)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        // 同时首次打开时只允许一份身份文件成功创建。
        match file.persist_noclobber(&path) {
            Ok(_) => Ok(device),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                Self::parse(&fs::read(path)?)
            }
            Err(error) => Err(error.error.into()),
        }
    }

    fn parse(bytes: &[u8]) -> Result<Self> {
        let device: Self = serde_json::from_slice(bytes).map_err(|_| Error::InvalidDevice)?;
        if Uuid::parse_str(&device.id).is_err() || device.name.trim().is_empty() {
            return Err(Error::InvalidDevice);
        }
        Ok(device)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Library;

    #[test]
    fn device_survives_reopening_and_is_separate_from_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::open(dir.path()).unwrap();
        let id = library.device().id.clone();
        let name = library.device().name.clone();
        assert!(Uuid::parse_str(&id).is_ok());
        assert!(!name.is_empty());
        drop(library);
        let reopened = Library::open(dir.path()).unwrap();
        assert_eq!(reopened.device().id, id);
        assert_eq!(reopened.device().name, name);
        drop(reopened);
        fs::remove_file(dir.path().join("library.db")).unwrap();
        assert_eq!(Library::open(dir.path()).unwrap().device().id, id);
    }

    #[test]
    fn corrupt_identity_is_not_silently_replaced() {
        let dir = tempfile::tempdir().unwrap();
        Library::open(dir.path()).unwrap();
        let path = dir.path().join("local/device.json");
        fs::write(&path, "{}").unwrap();
        assert!(matches!(
            Library::open(dir.path()),
            Err(Error::InvalidDevice)
        ));
        assert_eq!(fs::read_to_string(path).unwrap(), "{}");
    }
}
