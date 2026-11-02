//! 凭据仅存在系统钥匙串和进程内缓存；错误不携带底层平台的原文。
use sha2::{Digest, Sha256};
use sticker_core::sync::network::{Credentials, StorageConfig};

#[cfg(debug_assertions)]
pub const SERVICE: &str = "com.shaojian.sticker-manager.sync.debug";
#[cfg(not(debug_assertions))]
pub const SERVICE: &str = "com.shaojian.sticker-manager.sync";

pub fn account(device: &str, target: &StorageConfig) -> Result<String, String> {
    let normalized = target.normalized().map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(&normalized).map_err(|_| "无法识别同步目标".to_string())?;
    // Windows 的凭据名称有长度限制；摘要只用于稳定标识，不包含任何密钥。
    Ok(format!("{device}:{:x}", Sha256::digest(bytes)))
}

pub trait CredentialStore: Send + Sync {
    fn read(&self, account: &str) -> Result<Credentials, String>;
    fn save(&self, account: &str, credentials: &Credentials) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}
pub struct SystemCredentials;
fn entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account)
        .map_err(|_| "无法访问系统钥匙串，请允许访问后重新输入密码".into())
}
impl CredentialStore for SystemCredentials {
    fn read(&self, account: &str) -> Result<Credentials, String> {
        let value = entry(account)?
            .get_password()
            .map_err(|_| "无法读取已保存的凭据，请重新输入密码并允许访问钥匙串".to_string())?;
        Credentials::decode(&value).map_err(|e| e.to_string())
    }
    fn save(&self, account: &str, credentials: &Credentials) -> Result<(), String> {
        let value = credentials
            .encode()
            .map_err(|_| "无法保存凭据".to_string())?;
        entry(account)?
            .set_password(&value)
            .map_err(|_| "无法保存凭据，请允许访问系统钥匙串后重试".into())
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("无法删除已保存的凭据，请允许访问系统钥匙串后重试".into()),
        }
    }
}

/// 同一配置首次读取后缓存成功或失败；拒绝访问后也不会每五分钟再弹权限框。
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
    /// 读过钥匙串但没读到；还没读过时按已保存算。
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
        assert_eq!(SERVICE.ends_with(".debug"), cfg!(debug_assertions));
    }
}
