use super::*;
#[test]
fn relocate_snapshots_wal_and_private_local_files_before_cleanup() {
    let t = tempfile::tempdir().unwrap();
    let source = t.path().join("source");
    let target = t.path().join("target");
    let mut lib = Library::open(&source).unwrap();
    let device = lib.device.id.clone();
    let p = t.path().join("image.png");
    image::RgbaImage::new(2, 2).save(&p).unwrap();
    let id = lib.import_paths(&[p]).sticker_ids.pop().unwrap();
    for name in ["credentials.json", "settings.json"] {
        fs::write(source.join("local").join(name), b"local").unwrap()
    }
    fs::write(source.join("library-location.json"), b"pointer").unwrap();
    fs::write(source.join("unrelated"), b"keep").unwrap();
    fs::write(source.join("objects/.tmp-test"), b"temporary").unwrap();
    copy_content(&source, &target, &|| false, &mut |_| {}).unwrap();
    assert!(!target.join("objects/.tmp-test").exists());
    let category = lib.create_category("刚添加", None).unwrap();
    lib.set_sticker_category(&id, Some(&category.id)).unwrap();
    let old = lib
        .relocate(&target, &mut |p| {
            assert_eq!(p, target.canonicalize()?);
            Ok(())
        })
        .unwrap();
    assert_eq!(lib.root(), target.canonicalize().unwrap());
    assert_eq!(lib.device.id, device);
    let sticker = lib.sticker(&id).unwrap();
    assert_eq!(sticker.category.unwrap().name, "刚添加");
    assert!(sticker.object_path.is_file());
    for name in ["credentials.json", "settings.json"] {
        assert_eq!(fs::read(target.join("local").join(name)).unwrap(), b"local")
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            target
                .join("local/credentials.json")
                .metadata()
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        )
    }
    assert!(old.join("library.db").is_file());
    assert!(remove_content(&old).is_empty());
    assert_eq!(fs::read_dir(old).unwrap().count(), 2)
}
#[test]
fn commit_errors_leave_source_writable_and_copy_can_cancel() {
    let t = tempfile::tempdir().unwrap();
    let source = t.path().join("source");
    let target = t.path().join("target");
    let mut lib = Library::open(&source).unwrap();
    fs::write(source.join("objects/a"), "a").unwrap();
    fs::write(source.join("objects/b"), "b").unwrap();
    let n = std::cell::Cell::new(0);
    assert!(matches!(
        copy_content(&source, &target, &|| n.get() > 0, &mut |p| n.set(p.copied)).unwrap(),
        CopyOutcome::Cancelled
    ));
    copy_content(&source, &target, &|| false, &mut |_| {}).unwrap();
    assert!(
        lib.relocate(&target, &mut |_| Err(fail("pointer")))
            .is_err()
    );
    assert!(lib.relocate(&target, &mut |_| panic!()).is_err());
    assert_eq!(lib.root(), source.canonicalize().unwrap());
    lib.create_category("仍可写", None).unwrap();
    assert!(remove_content(&target).is_empty());
    assert_eq!(fs::read_dir(target).unwrap().count(), 0)
}
#[test]
fn target_validation() {
    let tmp = tempfile::tempdir().unwrap();
    let path = |s: &str| tmp.path().join(s);
    for d in ["cur/in", "base/in", "sync/in", "cloud/in", "empty"] {
        fs::create_dir_all(path(d)).unwrap()
    }
    let forbidden = [(path("cloud"), "cloud")];
    let rules = TargetRules {
        current_root: &path("cur"),
        default_root: &path("base"),
        pointer_name: "library-location.json",
        sync_folder: Some(&path("sync")),
        forbidden: &forbidden,
    };
    for p in [
        PathBuf::from("relative"),
        path("missing"),
        path("cur"),
        path("cur/in"),
        path("base/in"),
        tmp.path().into(),
        path("sync/in"),
        path("sync"),
        path("cloud/in"),
    ] {
        assert!(check_relocation_target(&p, &rules).is_err(), "{p:?}")
    }
    for n in [".DS_Store", "Thumbs.db", "desktop.ini"] {
        fs::write(path("empty").join(n), []).unwrap()
    }
    assert!(check_relocation_target(&path("empty"), &rules).is_ok());
    fs::write(path("empty/x"), []).unwrap();
    assert!(check_relocation_target(&path("empty"), &rules).is_err());
    assert!(check_relocation_target(&path("empty/x"), &rules).is_err());
    fs::remove_dir(path("base/in")).unwrap();
    fs::write(path("base/library-location.json"), []).unwrap();
    assert!(check_relocation_target(&path("base"), &rules).is_ok());
    fs::write(path("base/library.db"), []).unwrap();
    assert!(check_relocation_target(&path("base"), &rules).is_err())
}

#[test]
fn copy_skips_source_purged_after_enumeration() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    let target = tmp.path().join("target");
    let lib = Library::open(&source).unwrap();
    let image = lib.root().join("objects/gone.png");
    fs::write(&image, [1]).unwrap();
    let result = copy_content(&source, &target, &|| false, &mut |p| {
        if p.copied == 0 {
            fs::remove_file(&image).unwrap()
        }
    })
    .unwrap();
    assert!(matches!(result, CopyOutcome::Completed));
    assert!(!target.join("objects/gone.png").exists())
}

#[cfg(unix)]
#[test]
fn read_only_target_rejected() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    let target = tmp.path().join("readonly");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    let rules = TargetRules {
        current_root: &source,
        default_root: &source,
        pointer_name: "library-location.json",
        sync_folder: None,
        forbidden: &[],
    };
    fs::set_permissions(&target, fs::Permissions::from_mode(0o555)).unwrap();
    if File::create_new(target.join("probe")).is_err() {
        assert!(
            check_relocation_target(&target, &rules)
                .unwrap_err()
                .to_string()
                .contains("不能写入")
        )
    }
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
}
