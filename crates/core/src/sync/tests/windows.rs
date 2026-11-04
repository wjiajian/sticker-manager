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
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
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
