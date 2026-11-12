//! 凭据只存在本机仓库的 `local/credentials.json`（不同步）和进程内缓存；错误不携带底层平台的原文。
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

use sha2::{Digest, Sha256};
use sticker_core::sync::network::{Credentials, StorageConfig};
use tauri::{AppHandle, Manager};

use crate::settings::SettingsState;

pub fn account(device: &str, target: &StorageConfig) -> Result<String, String> {
    let normalized = target.normalized().map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(&normalized).map_err(|_| "无法识别同步目标".to_string())?;
    // 摘要只用于稳定标识，不包含任何密钥。
    Ok(format!("{device}:{:x}", Sha256::digest(bytes)))
}

pub trait CredentialStore: Send + Sync {
    fn read(&self, account: &str) -> Result<Credentials, String>;
    fn save(&self, account: &str, credentials: &Credentials) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// 账号到凭据的映射，整个文件一次写入；只有当前用户可以读写。
pub struct FileCredentials {
    path: PathBuf,
}
/// 保存和删除要先读后写，进程内串行。
static FILE_LOCK: Mutex<()> = Mutex::new(());

impl FileCredentials {
    pub fn new(root: &Path) -> Self {
        Self {
            path: root.join("local/credentials.json"),
        }
    }
    pub fn of(app: &AppHandle) -> Self {
        Self::new(&app.state::<SettingsState>().root)
    }
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "无法读取已保存的凭据，请重新输入密码".to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(_) => Err("无法读取已保存的凭据，请重新输入密码".into()),
        }
    }
    fn store(&self, accounts: &BTreeMap<String, String>) -> Result<(), String> {
        let failed = |_| "无法保存凭据，请检查仓库文件夹的写入权限后重试".to_string();
        let directory = self.path.parent().expect("凭据文件在 local 目录下");
        fs::create_dir_all(directory).map_err(failed)?;
        // 临时文件创建时权限就是 0600，改名后沿用。
        let mut file = tempfile::NamedTempFile::new_in(directory).map_err(failed)?;
        let bytes = serde_json::to_vec_pretty(accounts).map_err(|_| "无法保存凭据".to_string())?;
        file.write_all(&bytes)
            .and_then(|()| file.as_file().sync_all())
            .map_err(failed)?;
        file.persist(&self.path).map_err(|e| failed(e.error))?;
        Ok(())
    }
}
impl CredentialStore for FileCredentials {
    fn read(&self, account: &str) -> Result<Credentials, String> {
        let _guard = FILE_LOCK.lock().unwrap();
        let accounts = self.load()?;
        let value = accounts
            .get(account)
            .ok_or("没有已保存的凭据，请重新输入密码")?;
        Credentials::decode(value).map_err(|e| e.to_string())
    }
    fn save(&self, account: &str, credentials: &Credentials) -> Result<(), String> {
        let _guard = FILE_LOCK.lock().unwrap();
        let value = credentials
            .encode()
            .map_err(|_| "无法保存凭据".to_string())?;
        // 文件损坏时不保留旧内容，直接用新凭据重建。
        let mut accounts = self.load().unwrap_or_default();
        accounts.insert(account.into(), value);
        self.store(&accounts)
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        let _guard = FILE_LOCK.lock().unwrap();
        let mut accounts = self.load().map_err(|_| "无法删除已保存的凭据")?;
        if accounts.remove(account).is_none() {
            return Ok(());
        }
        if accounts.is_empty() {
            return fs::remove_file(&self.path).map_err(|_| "无法删除已保存的凭据".into());
        }
        self.store(&accounts)
    }
}

/// 同一配置首次读取后缓存成功或失败，每轮同步不必重读凭据文件。
#[derive(Default)]
pub struct CredentialCache {
    value: Option<(String, Result<Credentials, String>)>,
}
impl CredentialCache {
    pub fn read(
        &mut self,
        store: &dyn CredentialStore,
        account: &str,
    ) -> Result<Credentials, String> {
        if let Some((key, value)) = &self.value
            && key == account
        {
            return value.clone();
        }
        let result = store.read(account);
        self.value = Some((account.into(), result.clone()));
        result
    }
    pub fn remember(&mut self, account: String, credentials: Credentials) {
        self.value = Some((account, Ok(credentials)));
    }
    pub fn clear(&mut self) {
        self.value = None;
    }
    /// 读过凭据文件但没读到；还没读过时按已保存算。
    pub fn failed(&self) -> bool {
        matches!(self.value, Some((_, Err(_))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};
    #[derive(Default)]
    struct Mock {
        values: Mutex<HashMap<String, String>>,
        reads: Mutex<usize>,
    }
    impl CredentialStore for Mock {
        fn read(&self, account: &str) -> Result<Credentials, String> {
            *self.reads.lock().unwrap() += 1;
            let values = self.values.lock().unwrap();
            Credentials::decode(values.get(account).ok_or("请重新输入密码")?)
                .map_err(|e| e.to_string())
        }
        fn save(&self, account: &str, credentials: &Credentials) -> Result<(), String> {
            self.values
                .lock()
                .unwrap()
                .insert(account.into(), credentials.encode().unwrap());
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<(), String> {
            self.values.lock().unwrap().remove(account);
            Ok(())
        }
    }
    #[test]
    fn cache_reads_once_including_denial_and_instances_are_isolated() {
        let store = Mock::default();
        let target = StorageConfig::Webdav {
            url: "https://dav.example.com/test".into(),
        };
        let a = account("device-a", &target).unwrap();
        let b = account("device-b", &target).unwrap();
        let other = account(
            "device-a",
            &StorageConfig::Webdav {
                url: "https://dav.example.com/other".into(),
            },
        )
        .unwrap();
        assert_ne!(a, b);
        assert_ne!(a, other);
        let credentials = Credentials {
            password: "mock secret".into(),
            ..Credentials::default()
        };
        store.save(&a, &credentials).unwrap();
        store.save(&b, &credentials).unwrap();
        let mut cache = CredentialCache::default();
        for _ in 0..5 {
            assert_eq!(cache.read(&store, &a).unwrap().password, "mock secret");
        }
        assert_eq!(*store.reads.lock().unwrap(), 1);
        store.delete(&a).unwrap();
        cache.clear();
        assert!(!cache.failed());
        for _ in 0..5 {
            assert!(cache.read(&store, &a).err().unwrap().contains("重新输入"));
        }
        assert!(cache.failed());
        assert_eq!(*store.reads.lock().unwrap(), 2);
        assert_eq!(store.read(&b).unwrap().password, "mock secret");
        cache.remember(a.clone(), credentials);
        assert!(cache.read(&store, &a).is_ok());
    }
    #[test]
    fn file_store_round_trips_deletes_and_is_private() {
        let root = tempfile::tempdir().unwrap();
        let store = FileCredentials::new(root.path());
        let credentials = Credentials {
            username: "user".into(),
            password: "file secret".into(),
            ..Credentials::default()
        };
        assert!(store.read("a").is_err());
        assert!(store.delete("a").is_ok());
        store.save("a", &credentials).unwrap();
        store.save("b", &credentials).unwrap();
        assert_eq!(store.read("a").unwrap().password, "file secret");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(root.path().join("local/credentials.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0);
        }
        store.delete("a").unwrap();
        assert!(store.read("a").is_err());
        assert!(store.read("b").is_ok());
        store.delete("b").unwrap();
        assert!(!root.path().join("local/credentials.json").exists());
        fs::create_dir_all(root.path().join("local")).unwrap();
        fs::write(root.path().join("local/credentials.json"), "乱码{").unwrap();
        assert!(store.read("a").is_err());
        store.save("a", &credentials).unwrap();
        assert!(store.read("a").is_ok());
    }
}
