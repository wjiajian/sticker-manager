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
fn behavior(kind: &str) {
    let storage = storage(kind);
    probe(&storage).unwrap();
    assert!(storage.list("").unwrap().is_empty());
    assert!(storage.list("ops/").unwrap().is_empty());
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
    assert_eq!(
        storage.list("nested/").unwrap(),
        vec![
            storage::FileEntry {
                key: "nested/first".into(),
                size: 11
            },
            storage::FileEntry {
                key: "nested/中文/second".into(),
                size: 6
            }
        ]
    );
    for file in storage.list("").unwrap() {
        storage.delete(&file.key).unwrap();
    }
    two_library_end_to_end(&storage);
    for file in storage.list("").unwrap() {
        storage.delete(&file.key).unwrap();
    }
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
    assert_eq!(storage.list("objects/").unwrap().len(), 3);
    a.sync(&storage, &manifest);
    b.sync(&storage, &manifest);
    assert_eq!(a.state(), b.state());
    assert_eq!(storage.list("objects/").unwrap().len(), 20);
    for file in storage.list("").unwrap() {
        storage.delete(&file.key).unwrap();
    }
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
