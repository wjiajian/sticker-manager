use super::*;
use crate::LibraryView;
use tempfile::TempDir;
fn library() -> (TempDir, Library, Vec<String>) {
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(&tmp.path().join("source")).unwrap();
    let mut ids = Vec::new();
    for n in 0..3 {
        let p = tmp.path().join(format!("{n}.png"));
        image::RgbaImage::from_pixel(2, 2, image::Rgba([n, 20, 30, 255]))
            .save(&p)
            .unwrap();
        ids.extend(lib.import_paths(&[p]).sticker_ids);
    }
    let cat = lib.create_category("猫:?", None).unwrap();
    for id in &ids[..2] {
        lib.set_sticker_category(id, Some(&cat.id)).unwrap();
        lib.set_sticker_name(id, "同名").unwrap()
    }
    (tmp, lib, ids)
}
fn request(parent: &Path, format: ExportFormat) -> ExportRequest {
    ExportRequest {
        scope: ExportScope::Library,
        format,
        destination: if format == ExportFormat::Zip {
            parent.join("导出.zip")
        } else {
            parent.into()
        },
        name: "仓库".into(),
        forbidden: vec![],
        utc_offset_minutes: 480,
    }
}
#[test]
fn component_names_are_portable_and_unique() {
    for (raw, want) in [
        ("../收到:好的?", "_收到_好的_"),
        (" .. ", "表情"),
        ("CON", "_CON"),
        ("con.txt", "_con.txt"),
        ("COM1", "_COM1"),
        ("COM¹", "_COM¹"),
        ("LPT9", "_LPT9"),
        (".hidden", "hidden"),
        ("a..b. ", "a.b"),
        ("a\tb", "a_b"),
    ] {
        assert_eq!(sanitize_component(raw, "表情"), want)
    }
    assert_eq!(sanitize_component(&"字".repeat(100), "表情").len(), 120);
    let mut names = NameSet::default();
    assert_eq!(names.claim("a", ".png"), "a.png");
    assert_eq!(names.claim("A (2)", ".png"), "A (2).png");
    assert_eq!(names.claim("A", ".png"), "A (3).png");
    names.0.insert("未分类".into());
    assert_eq!(names.claim("未分类", ""), "未分类 (2)");
}
#[test]
fn folder_and_zip_roundtrip_originals_and_categories() {
    let (tmp, lib, _) = library();
    fs::create_dir(tmp.path().join("仓库")).unwrap();
    for fmt in [ExportFormat::Folder, ExportFormat::Zip] {
        let r = lib.export(&request(tmp.path(), fmt)).unwrap();
        assert_eq!(r.exported, 3);
        let output = r.output.unwrap();
        // Windows 上文件夹和压缩包都返回不带 \\?\ 前缀的路径，界面显示一致。
        assert_eq!(output, crate::relocate::display_path(&output));
        #[cfg(windows)]
        assert!(!output.to_string_lossy().starts_with(r"\\?\"));
        if fmt == ExportFormat::Folder {
            assert!(output.ends_with("仓库 (2)"));
            assert!(output.join("猫__/同名 (2).png").is_file());
            assert!(output.join("未分类/2.png").is_file())
        } else {
            let mut archive = zip::ZipArchive::new(File::open(&output).unwrap()).unwrap();
            for name in ["猫__/同名.png", "猫__/同名 (2).png", "未分类/2.png"] {
                let mut f = archive.by_name(name).unwrap();
                assert_eq!(f.compression(), zip::CompressionMethod::Stored);
                let mut bytes = Vec::new();
                f.read_to_end(&mut bytes).unwrap();
                assert!(!bytes.is_empty())
            }
        }
        let mut imported = Library::open(&tmp.path().join(format!("{fmt:?}"))).unwrap();
        assert_eq!(imported.import_paths(&[output]).added, 3);
        for s in lib.list_stickers(LibraryView::All).unwrap() {
            let got = imported
                .list_stickers(LibraryView::All)
                .unwrap()
                .into_iter()
                .find(|i| i.object_path.file_name() == s.object_path.file_name())
                .unwrap();
            assert_eq!(
                fs::read(got.object_path).unwrap(),
                fs::read(s.object_path).unwrap()
            );
            assert_eq!(
                got.category.map(|c| c.name),
                s.category.map(|c| sanitize_component(&c.name, "分类"))
            )
        }
    }
}
#[test]
fn selected_category_missing_and_forbidden() {
    let (tmp, lib, ids) = library();
    let mut r = request(tmp.path(), ExportFormat::Folder);
    let category = lib.sticker(&ids[0]).unwrap().category.unwrap();
    r.scope = ExportScope::Category(category.id);
    let dir = lib.export(&r).unwrap().output.unwrap();
    assert!(dir.join("同名.png").is_file());
    assert!(dir.ends_with("猫__"));
    r.format = ExportFormat::Zip;
    r.destination = tmp.path().join("category");
    let out = lib.export(&r).unwrap().output.unwrap();
    assert!(out.ends_with("category.zip"));
    assert!(lib.export(&r).is_err());
    assert!(
        zip::ZipArchive::new(File::open(out).unwrap())
            .unwrap()
            .by_name("猫__/同名.png")
            .is_ok()
    );
    lib.delete_sticker(&ids[0]).unwrap();
    r = request(tmp.path(), ExportFormat::Folder);
    r.scope =
        ExportScope::Stickers([ids.clone(), vec!["unknown".into(), "unknown".into()]].concat());
    fs::remove_file(lib.sticker(&ids[1]).unwrap().object_path).unwrap();
    let report = lib.export(&r).unwrap();
    assert_eq!(
        (report.skipped, report.exported, report.failures.len()),
        (2, 1, 1)
    );
    fs::remove_file(lib.sticker(&ids[2]).unwrap().object_path).unwrap();
    assert!(lib.export(&r).unwrap().output.is_none());
    r.destination = lib.root.clone();
    assert!(lib.export(&r).is_err());
    r.destination = tmp.path().into();
    r.forbidden.push(tmp.path().into());
    assert!(lib.export(&r).is_err());
    r.scope = ExportScope::Stickers(vec![]);
    assert!(lib.export(&r).is_err());
}
#[test]
fn cancellation_cleans_and_callbacks_can_lock() {
    let (tmp, lib, _) = library();
    let lib = Mutex::new(lib);
    for fmt in [ExportFormat::Folder, ExportFormat::Zip] {
        let r = request(tmp.path(), fmt);
        if fmt == ExportFormat::Zip {
            fs::write(&r.destination, b"previous").unwrap()
        }
        let cancel = std::cell::Cell::new(false);
        let report = export_with_progress(&lib, &r, &|| cancel.get(), &mut |_| {
            assert!(lib.try_lock().is_ok());
            cancel.set(true)
        })
        .unwrap();
        assert!(report.cancelled && report.output.is_none());
        assert!(!tmp.path().join("仓库").exists());
        if fmt == ExportFormat::Zip {
            assert_eq!(fs::read(&r.destination).unwrap(), b"previous");
            assert!(!fs::read_dir(tmp.path()).unwrap().flatten().any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".sticker-export-")
            }))
        }
    }
}
#[test]
fn timestamps_and_import_limits() {
    let t = zip_time(1_704_067_200_000, 480);
    assert_eq!((t.year(), t.month(), t.day(), t.hour()), (2024, 1, 1, 8));
    assert_eq!(zip_time(0, 0), zip::DateTime::default());
    assert!(!exceeds_limits(10000, ARCHIVE_LIMIT, ENTRY_LIMIT));
    assert!(
        exceeds_limits(10001, 0, 0)
            && exceeds_limits(1, ARCHIVE_LIMIT + 1, 1)
            && exceeds_limits(1, 1, ENTRY_LIMIT + 1)
    )
}
