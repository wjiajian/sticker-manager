use super::*;

#[test]
fn incompatible_folder_names_fail_before_creating_files() {
    let directory = tempfile::tempdir().unwrap();
    let folder = FolderStorage::new(directory.path()).unwrap();
    for name in [
        "CON", "nul", "Aux.txt", "COM1", "LPT²", "末尾.", "末尾 ", "问?号", "星*号", "冒:号",
        "小<于", "大>于", "引\"号", "管|道",
    ] {
        let key = format!("正常分类/{name}/收到.png");
        let error = folder.write(&key, b"unchanged").unwrap_err().to_string();
        assert!(error.contains("Windows"), "{key}: {error}");
        assert!(error.contains(name), "{key}: {error}");
        // 未发布成功的旧路径不应阻止用户改名后重试。
        assert!(folder.read(&key).unwrap().is_none(), "{key}");
        folder.delete(&key).unwrap();
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn overlong_missing_paths_do_not_block_rename_retries() {
    let directory = tempfile::tempdir().unwrap();
    let folder = FolderStorage::new(directory.path()).unwrap();
    let key = format!("{}/收到.png", "长".repeat(256));
    assert!(folder.write(&key, b"original").is_err());
    assert!(folder.read(&key).unwrap().is_none());
    folder.delete(&key).unwrap();
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn existing_reserved_paths_can_be_read_and_removed_during_a_rename() {
    let directory = tempfile::tempdir().unwrap();
    let folder = FolderStorage::new(directory.path()).unwrap();
    // verbatim 路径可访问外部程序已创建的保留名文件夹。
    fs::create_dir(folder.root().join("CON")).unwrap();
    fs::write(folder.root().join("CON/收到.png"), b"original").unwrap();
    assert_eq!(folder.read("CON/收到.png").unwrap().unwrap(), b"original");
    folder.write("兼容分类/收到.png", b"original").unwrap();
    folder.delete("CON/收到.png").unwrap();
    folder.delete("CON/").unwrap();
    assert_eq!(
        folder.read("兼容分类/收到.png").unwrap().unwrap(),
        b"original"
    );
}

#[test]
fn unicode_folder_names_and_network_names_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let folder = FolderStorage::new(directory.path()).unwrap();
    let key = "收到😊/好的.png";
    folder.write(key, b"original").unwrap();
    assert_eq!(folder.read(key).unwrap().unwrap(), b"original");
    assert!(
        folder
            .list("")
            .unwrap()
            .iter()
            .any(|entry| entry.key == key)
    );

    let network = MemoryStorage::default();
    let network_key = "冒:号/收到?.png";
    network.write(network_key, b"original").unwrap();
    assert_eq!(network.read(network_key).unwrap().unwrap(), b"original");
    assert!(super::super::storage::validate_key(network_key).is_ok());
}

#[test]
fn incompatible_destination_is_rejected_before_journaling_and_renaming_recovers() {
    let site = Site::new();
    let directory = tempfile::tempdir().unwrap();
    let folder = FolderStorage::new(directory.path()).unwrap();
    let manifest = Manifest::create(&folder).unwrap();
    let id = site.import(77);
    let category = site.lib.lock().unwrap().categorize(&id, "CON").unwrap();
    assert!(synchronize(&site.lib, &folder, &manifest.id).is_err());
    let pending: i64 = site
        .lib
        .lock()
        .unwrap()
        .conn
        .query_row("SELECT count(*) FROM sync_file_moves", [], |row| row.get(0))
        .unwrap();
    assert_eq!(pending, 0);
    site.lib
        .lock()
        .unwrap()
        .rename_category(&category.id, "兼容分类")
        .unwrap();
    site.sync(&folder, &manifest);
    assert!(folder.read("兼容分类/77.png").unwrap().is_some());
}

#[test]
fn renaming_replans_an_incompatible_destination_left_by_an_older_client() {
    // 模拟只有写入阶段校验名称的旧客户端。
    struct Legacy(FolderStorage);
    impl Storage for Legacy {
        fn list(&self, prefix: &str) -> Result<Vec<storage::FileEntry>> {
            self.0.list(prefix)
        }
        fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
            self.0.read(key)
        }
        fn write(&self, key: &str, bytes: &[u8]) -> Result<()> {
            self.0.write(key, bytes)
        }
        fn delete(&self, key: &str) -> Result<()> {
            self.0.delete(key)
        }
    }
    let site = Site::new();
    let directory = tempfile::tempdir().unwrap();
    let legacy = Legacy(FolderStorage::new(directory.path()).unwrap());
    let manifest = Manifest::create(&legacy).unwrap();
    let id = site.import(78);
    let category = site.lib.lock().unwrap().categorize(&id, "CON").unwrap();
    assert!(synchronize(&site.lib, &legacy, &manifest.id).is_err());
    let pending: i64 = site
        .lib
        .lock()
        .unwrap()
        .conn
        .query_row("SELECT count(*) FROM sync_file_moves", [], |row| row.get(0))
        .unwrap();
    assert!(pending > 0);
    site.lib
        .lock()
        .unwrap()
        .rename_category(&category.id, "兼容分类")
        .unwrap();
    site.sync(&legacy.0, &manifest);
    assert!(legacy.0.read("兼容分类/78.png").unwrap().is_some());
    assert_eq!(remote_images(&legacy.0).len(), 1);
}
