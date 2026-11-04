//! 本地容器测试；所有地址/凭据从环境变量读，默认忽略。
use super::*;
use crate::sync::network::{Credentials, NetworkStorage, StorageConfig, probe};
use std::{env, sync::Arc};
fn required(key: &str) -> String {
    env::var(key).unwrap_or_else(|_| panic!("请设置 {key}"))
}
fn storage(kind: &str) -> NetworkStorage {
    let root = format!("test-{}/中文 路径", Uuid::new_v4());
    let (target, credentials) = if kind == "webdav" {
        (
            StorageConfig::Webdav {
                url: format!(
                    "{}/{}",
                    required("SYNC_WEBDAV_URL").trim_end_matches('/'),
                    root
                ),
            },
            Credentials {
                username: required("SYNC_WEBDAV_USER"),
                password: required("SYNC_WEBDAV_PASSWORD"),
                ..Credentials::default()
            },
        )
    } else {
        (
            StorageConfig::S3 {
                endpoint: required("SYNC_S3_ENDPOINT"),
                region: required("SYNC_S3_REGION"),
                bucket: required("SYNC_S3_BUCKET"),
                path: root,
            },
            Credentials {
                access_key_id: required("SYNC_S3_ACCESS_KEY_ID"),
                secret_access_key: required("SYNC_S3_SECRET_ACCESS_KEY"),
                ..Credentials::default()
            },
        )
    };
    NetworkStorage::new(&target, &credentials, Arc::new(|| false)).unwrap()
}
fn cleanup(storage: &dyn Storage) {
    let mut entries = storage.list("").unwrap();
    entries.sort_by_key(|e| std::cmp::Reverse(e.key.len()));
    for entry in entries {
        storage.delete(&entry.key).unwrap();
    }
}
fn behavior(kind: &str) {
    let storage = storage(kind);
    probe(&storage).unwrap();
    assert!(storage.list("").unwrap().is_empty());
    assert!(storage.list(".sticker-manager/ops/").unwrap().is_empty());
    assert_eq!(storage.read("missing").unwrap(), None);
    storage.delete("missing").unwrap();
    for key in ["", "/absolute", "../outside", "a/../b", "a\\b"] {
        assert!(storage.read(key).is_err());
        assert!(storage.write(key, b"x").is_err());
        assert!(storage.delete(key).is_err());
    }
    storage.write("nested/中文/second", b"second").unwrap();
    storage.write("nested/first", b"first").unwrap();
    storage.write("nested/first", b"replacement").unwrap();
    assert_eq!(
        storage.read("nested/first").unwrap().unwrap(),
        b"replacement"
    );
    let entries = storage
        .list("nested/")
        .unwrap()
        .into_iter()
        .filter(|f| !f.key.ends_with('/'))
        .collect::<Vec<_>>();
    assert_eq!(
        entries
            .iter()
            .map(|f| (f.key.as_str(), f.size))
            .collect::<Vec<_>>(),
        [("nested/first", 11), ("nested/中文/second", 6)]
    );
    assert!(entries.iter().all(|f| f.version.is_some()));
    cleanup(&storage);
    two_library_end_to_end(&storage);
    cleanup(&storage);
}
#[test]
#[ignore = "需要 SYNC_WEBDAV_* 环境变量和本地 WebDAV 容器"]
fn webdav_storage_and_two_device_sync() {
    behavior("webdav");
}
#[test]
#[ignore = "需要 SYNC_S3_* 环境变量和本地 MinIO 容器"]
fn s3_storage_and_two_device_sync() {
    behavior("s3");
}

fn outage(kind: &str) {
    let container = required(if kind == "webdav" {
        "SYNC_WEBDAV_CONTAINER"
    } else {
        "SYNC_S3_CONTAINER"
    });
    let storage = storage(kind);
    let manifest = Manifest::create(&storage).unwrap();
    let a = Site::new();
    let b = Site::new();
    for color in 100..120 {
        a.import(color);
    }
    let before = a.state();
    let stopped = std::sync::atomic::AtomicBool::new(false);
    let report = synchronize_with_progress(&a.lib, &storage, &manifest.id, &|p| {
        if matches!(p.stage, TransferStage::Uploading)
            && p.completed == 3
            && !stopped.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            assert!(
                std::process::Command::new("docker")
                    .args(["stop", "-t", "0", &container])
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
    });
    // 无论断线结果如何都先恢复测试容器。
    assert!(
        std::process::Command::new("docker")
            .args(["start", &container])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(report.is_err());
    assert_eq!(a.state(), before);
    assert_eq!(remote_images(&storage).len(), 3);
    a.sync(&storage, &manifest);
    b.sync(&storage, &manifest);
    assert_eq!(a.state(), b.state());
    assert_eq!(remote_images(&storage).len(), 20);
    cleanup(&storage);
}
#[test]
#[ignore = "停止并重启 SYNC_WEBDAV_CONTAINER，只用于专用测试容器"]
fn webdav_disconnect_resumes_without_local_damage() {
    outage("webdav");
}
#[test]
#[ignore = "停止并重启 SYNC_S3_CONTAINER，只用于专用测试容器"]
fn s3_disconnect_resumes_without_local_damage() {
    outage("s3");
}

fn log_compaction_roundtrip(kind: &str) {
    let storage = storage(kind);
    let manifest = Manifest::create(&storage).unwrap();
    let a = Site::new();
    let b = Site::new();
    let id = a.import(223);
    for i in 0..32 {
        a.lib
            .lock()
            .unwrap()
            .set_sticker_caption(&id, &format!("合并第 {i} 轮"))
            .unwrap();
        a.sync(&storage, &manifest);
    }
    let device = a.lib.lock().unwrap().device.id.clone();
    let files = storage
        .list(&format!(".sticker-manager/ops/{device}/"))
        .unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].key.ends_with("000000000001-000000000033.jsonl"));
    b.sync(&storage, &manifest);
    assert_eq!(a.state(), b.state());
    a.lib
        .lock()
        .unwrap()
        .set_sticker_caption(&id, "合并之后继续推送")
        .unwrap();
    a.sync(&storage, &manifest);
    b.sync(&storage, &manifest);
    assert_eq!(a.state(), b.state());
    cleanup(&storage);
}
#[test]
#[ignore = "需要 SYNC_WEBDAV_* 环境变量和本地 WebDAV 容器"]
fn webdav_log_compaction_roundtrip() {
    log_compaction_roundtrip("webdav");
}
#[test]
#[ignore = "需要 SYNC_S3_* 环境变量和本地 MinIO 容器"]
fn s3_log_compaction_roundtrip() {
    log_compaction_roundtrip("s3");
}
