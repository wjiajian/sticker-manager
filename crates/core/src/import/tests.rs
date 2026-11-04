use super::*;
use crate::LibraryView;
use image::{Frame, Rgba, RgbaImage, codecs::gif::GifEncoder};

fn png_file(path: &Path, color: [u8; 4]) {
    RgbaImage::from_pixel(400, 200, Rgba(color))
        .save(path)
        .unwrap();
}

#[test]
fn deduplicates_content_and_owns_original_and_thumbnail() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let source = dir.path().join("收到.png");
    png_file(&source, [255, 0, 0, 128]);
    let copy = dir.path().join("另一个名字.jpg");
    fs::copy(&source, &copy).unwrap();
    let bytes = fs::read(&source).unwrap();
    let report = library.import_paths(&[source.clone(), copy]);
    assert_eq!((report.added, report.existing, report.skipped), (1, 1, 0));
    assert!(report.failures.is_empty());

    let stickers = library.list_stickers(LibraryView::All).unwrap();
    let sticker = &stickers[0];
    assert!(Uuid::parse_str(&sticker.id).is_ok());
    assert_eq!(sticker.name, "收到");
    assert_eq!((sticker.width, sticker.height), (400, 200));
    assert_eq!(sticker.byte_size, bytes.len() as i64);
    assert!(!sticker.animated);
    assert!(sticker.added_at > 0);
    assert_eq!(
        sticker.object_path.file_name().unwrap().to_str().unwrap(),
        format!("{:x}.png", Sha256::digest(&bytes))
    );
    fs::remove_file(source).unwrap();
    assert_eq!(fs::read(&sticker.object_path).unwrap(), bytes);
    let thumbnail = image::open(&sticker.thumbnail_path).unwrap().to_rgba8();
    assert_eq!(thumbnail.dimensions(), (256, 128));
    assert_eq!(thumbnail.get_pixel(0, 0).0[3], 128);
    assert_eq!(
        fs::read_dir(library.root.join("objects")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(library.root.join("thumbs")).unwrap().count(),
        1
    );

    let reopened = Library::open(&library.root).unwrap();
    assert_eq!(
        reopened.list_stickers(LibraryView::All).unwrap()[0].id,
        sticker.id
    );
}

#[test]
fn restores_deleted_sticker_without_losing_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let source = dir.path().join("收到.png");
    png_file(&source, [255, 0, 0, 255]);
    library.import_paths(std::slice::from_ref(&source));
    let before = library.list_stickers(LibraryView::All).unwrap().remove(0);
    library.conn.execute_batch(
        "UPDATE stickers SET name = '手动名称', caption = '配文', favorite = 1, deleted_at = 10;
         INSERT INTO categories (id, name, color, position) VALUES ('t', '开心', '#F2B705', 0);
         UPDATE stickers SET category_id = 't';
         INSERT INTO usages SELECT id, 'mac', 3, 20 FROM stickers;"
    ).unwrap();

    let report = library.import_paths(std::slice::from_ref(&source));
    assert_eq!((report.added, report.existing, report.restored), (0, 1, 1));
    assert!(report.failures.is_empty());
    let after = library.list_stickers(LibraryView::All).unwrap().remove(0);
    assert_eq!(before.id, after.id);
    assert_eq!(before.added_at, after.added_at);
    assert_eq!(after.name, "手动名称");
    let overview = library.overview().unwrap();
    assert_eq!(
        (
            overview.all,
            overview.favorites,
            overview.recent,
            overview.uncategorized,
            overview.recently_deleted
        ),
        (1, 1, 1, 0, 0)
    );
    let caption: String = library
        .conn
        .query_row("SELECT caption FROM stickers", [], |row| row.get(0))
        .unwrap();
    assert_eq!(caption, "配文");

    library.conn.execute("DELETE FROM stickers", []).unwrap();
    assert_eq!(library.import_paths(&[source]).added, 1);
    assert_ne!(
        before.id,
        library.list_stickers(LibraryView::All).unwrap()[0].id
    );
    assert_eq!(library.overview().unwrap().favorites, 0);
}

#[test]
fn recurses_and_counts_unsupported_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let folder = dir.path().join("表情包");
    fs::create_dir_all(folder.join("子目录/更深一层")).unwrap();
    png_file(&folder.join("一.png"), [255, 0, 0, 255]);
    png_file(&folder.join("子目录/更深一层/二.png"), [0, 255, 0, 255]);
    fs::write(folder.join("说明.txt"), "说明").unwrap();
    fs::write(folder.join("表情包.zip"), b"PK\x03\x04").unwrap();
    fs::write(folder.join("视频.mp4"), b"\0\0\0\x18ftypmp42").unwrap();
    fs::write(folder.join("子目录/vector.svg"), b"<svg/>").unwrap();
    let report = library.import_paths(&[folder]);
    assert_eq!((report.added, report.skipped), (2, 4));
    assert!(report.failures.is_empty());
    assert_eq!(library.overview().unwrap().all, 2);
}

#[test]
fn ignores_hidden_and_system_files_in_folders() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let folder = dir.path().join("表情包");
    fs::create_dir_all(folder.join(".隐藏目录")).unwrap();
    png_file(&folder.join("一.png"), [255, 0, 0, 255]);
    png_file(&folder.join("._一.png"), [0, 255, 0, 255]);
    png_file(&folder.join(".隐藏目录/二.png"), [0, 0, 255, 255]);
    fs::write(folder.join(".DS_Store"), b"\0\0\0\x01Bud1").unwrap();
    fs::write(folder.join("Thumbs.db"), b"\xd0\xcf\x11\xe0").unwrap();
    fs::write(folder.join("desktop.ini"), "[.ShellClassInfo]").unwrap();
    let report = library.import_paths(&[folder]);
    assert_eq!((report.added, report.skipped), (1, 0));
    assert!(report.failures.is_empty());

    // 用户直接选中的隐藏文件照常导入
    let chosen = dir.path().join(".单独选中.png");
    png_file(&chosen, [9, 9, 9, 255]);
    assert_eq!(library.import_paths(&[chosen]).added, 1);
}

#[test]
fn continues_after_corrupt_or_missing_file_and_cleans_temporary_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let corrupt = dir.path().join("损坏.png");
    let good = dir.path().join("收到.png");
    fs::write(&corrupt, b"\x89PNG\r\n\x1a\ntruncated").unwrap();
    png_file(&good, [1, 2, 3, 255]);
    let report = library.import_paths(&[corrupt, dir.path().join("不存在.png"), good]);
    assert_eq!((report.added, report.failures.len()), (1, 2));
    assert_eq!(
        fs::read_dir(library.root.join("objects")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(library.root.join("thumbs")).unwrap().count(),
        1
    );
}

#[test]
fn repairs_missing_files_when_importing_existing_sticker() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let source = dir.path().join("收到.png");
    png_file(&source, [1, 2, 3, 255]);
    library.import_paths(std::slice::from_ref(&source));
    let sticker = library.list_stickers(LibraryView::All).unwrap().remove(0);
    fs::remove_file(&sticker.thumbnail_path).unwrap();
    fs::remove_file(&sticker.object_path).unwrap();
    let report = library.import_paths(&[source]);
    assert_eq!((report.added, report.existing), (0, 1));
    assert!(report.failures.is_empty());
    assert!(sticker.thumbnail_path.is_file());
    assert!(sticker.object_path.is_file());
}

#[test]
fn existing_complete_sticker_skips_decoding_but_a_purge_before_commit_still_gets_a_thumbnail() {
    // 准备阶段说“已有且文件齐全”，提交时库里却已经没有这张：要在锁内补做解码。
    struct Stale<'a>(&'a mut Library);
    impl Store for Stale<'_> {
        fn has_complete_files(&mut self, _: &str, _: &str) -> Result<bool> {
            Ok(true)
        }
        fn commit(&mut self, prepared: Prepared, report: &mut ImportReport) -> Result<()> {
            assert!(prepared.thumbnail.is_none());
            self.0.commit_import(prepared, report)
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let source = dir.path().join("收到.png");
    png_file(&source, [9, 9, 9, 255]);
    let root = library.root.clone();
    let report = run(
        &root,
        std::slice::from_ref(&source),
        &|| false,
        &mut |_| {},
        &mut Stale(&mut library),
    );
    assert_eq!((report.added, report.failures.len()), (1, 0), "{report:?}");
    let sticker = library.list_stickers(LibraryView::All).unwrap().remove(0);
    assert_eq!((sticker.width, sticker.height), (400, 200));
    assert!(sticker.thumbnail_path.is_file());

    struct Counting<'a>(&'a mut Library, usize);
    impl Store for Counting<'_> {
        fn has_complete_files(&mut self, hash: &str, ext: &str) -> Result<bool> {
            self.0.has_complete_files(hash, ext)
        }
        fn commit(&mut self, prepared: Prepared, report: &mut ImportReport) -> Result<()> {
            self.1 += usize::from(prepared.thumbnail.is_some());
            self.0.commit_import(prepared, report)
        }
    }
    let mut store = Counting(&mut library, 0);
    let report = run(&root, &[source], &|| false, &mut |_| {}, &mut store);
    assert_eq!((report.existing, store.1), (1, 0));
}

#[test]
fn file_write_failure_does_not_leave_a_record_or_original() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let source = dir.path().join("收到.png");
    png_file(&source, [1, 2, 3, 255]);
    let hash = format!("{:x}", Sha256::digest(fs::read(&source).unwrap()));
    fs::create_dir(library.root.join("thumbs").join(format!("{hash}.png"))).unwrap();
    let report = library.import_paths(&[source]);
    assert_eq!((report.added, report.failures.len()), (0, 1));
    assert_eq!(library.overview().unwrap().all, 0);
    assert_eq!(
        fs::read_dir(library.root.join("objects")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(library.root.join("thumbs")).unwrap().count(),
        1
    );
}

#[test]
fn imports_jpeg_webp_gif_and_apng_with_animation_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let source = dir.path().join("输入");
    fs::create_dir(&source).unwrap();
    let rgba = RgbaImage::from_pixel(8, 4, Rgba([12, 34, 56, 255]));
    DynamicImage::ImageRgba8(rgba.clone())
        .to_rgb8()
        .save(source.join("静态.jpg"))
        .unwrap();
    rgba.save(source.join("静态.webp")).unwrap();
    for (name, count) in [("静态.gif", 1), ("动图.gif", 2)] {
        let mut encoder = GifEncoder::new(File::create(source.join(name)).unwrap());
        for _ in 0..count {
            encoder.encode_frame(Frame::new(rgba.clone())).unwrap();
        }
    }
    let mut encoder = png::Encoder::new(File::create(source.join("动图.apng")).unwrap(), 8, 4);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_animated(2, 0).unwrap();
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(rgba.as_raw()).unwrap();
    writer.write_image_data(rgba.as_raw()).unwrap();
    writer.finish().unwrap();
    // 两帧 8 × 4 纯色 WebP，由 Pillow 生成。
    fs::write(
        source.join("动图.webp"),
        include_bytes!("fixtures/animated.webp"),
    )
    .unwrap();

    let report = library.import_paths(&[source]);
    assert_eq!(report.added, 6, "{report:?}");
    assert!(report.failures.is_empty());
    for sticker in library.list_stickers(LibraryView::All).unwrap() {
        assert_eq!((sticker.width, sticker.height), (8, 4));
        assert_eq!(sticker.animated, sticker.name == "动图");
        let first_frame = sticker.read_image().unwrap();
        assert_eq!((first_frame.width(), first_frame.height()), (8, 4));
        assert_eq!(image::open(sticker.thumbnail_path).unwrap().width(), 8);
    }
}

#[cfg(unix)]
#[test]
fn skips_symlink_cycles_and_the_library_itself() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    png_file(&dir.path().join("收到.png"), [1, 2, 3, 255]);
    std::os::unix::fs::symlink(dir.path(), dir.path().join("循环")).unwrap();
    let report = library.import_paths(&[dir.path().to_path_buf()]);
    assert_eq!((report.added, report.skipped), (1, 2));
    assert!(report.failures.is_empty());
}

pub(super) fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}
fn patch_zip_headers(path: &Path, local: usize, central: usize, value: u16) {
    let mut bytes = fs::read(path).unwrap();
    for i in 0..bytes.len().saturating_sub(46) {
        let offset = if bytes[i..].starts_with(b"PK\x03\x04") {
            Some(local)
        } else if bytes[i..].starts_with(b"PK\x01\x02") {
            Some(central)
        } else {
            None
        };
        if let Some(offset) = offset {
            bytes[i + offset..i + offset + 2].copy_from_slice(&value.to_le_bytes());
        }
    }
    fs::write(path, bytes).unwrap();
}
#[test]
fn zip_recurses_ignores_hidden_and_nested_archives_and_deduplicates() {
    let dir = tempfile::tempdir().unwrap();
    let library = Mutex::new(Library::open(&dir.path().join("library")).unwrap());
    let image = dir.path().join("原图.png");
    png_file(&image, [9, 8, 7, 255]);
    let bytes = fs::read(&image).unwrap();
    let archive = dir.path().join("表情包.zip");
    make_zip(
        &archive,
        &[
            ("子目录/中文.png", &bytes),
            ("重复.png", &bytes),
            ("__MACOSX/a.png", &bytes),
            ("deep/.隐藏/x.png", &bytes),
            ("deep/Thumbs.db", &bytes),
            ("deep/desktop.ini", &bytes),
            ("deep/nested.zip", &bytes),
            ("说明.txt", b"text"),
            ("目录/", b""),
        ],
    );
    let mut updates = Vec::new();
    let report = import_paths_with_progress(
        &library,
        std::slice::from_ref(&archive),
        &|| false,
        &mut |p| updates.push(p),
    )
    .unwrap();
    assert_eq!((report.added, report.existing, report.skipped), (1, 1, 2));
    assert_eq!(report.sticker_ids.len(), 1);
    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(updates.last().map(|p| (p.processed, p.total)), Some((9, 9)));
    assert!(updates.windows(2).all(|w| w[0].processed <= w[1].processed));
    assert_eq!(
        library
            .lock()
            .unwrap()
            .sticker(&report.sticker_ids[0])
            .unwrap()
            .name,
        "中文"
    );
    let again = import_paths_with_progress(&library, &[archive], &|| false, &mut |_| {}).unwrap();
    assert_eq!((again.added, again.existing), (0, 2));
}
#[test]
fn zip_gbk_and_unflagged_utf8_names_are_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let image = dir.path().join("image.png");
    png_file(&image, [6, 5, 4, 255]);
    let data = fs::read(image).unwrap();
    let gbk = dir.path().join("gbk.zip");
    make_zip(&gbk, &[("1234.png", &data)]);
    let mut bytes = fs::read(&gbk).unwrap();
    let (name, _, _) = encoding_rs::GB18030.encode("中文.png");
    let positions: Vec<_> = bytes
        .windows(8)
        .enumerate()
        .filter(|(_, w)| *w == b"1234.png")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(positions.len(), 2);
    for i in positions {
        bytes[i..i + 8].copy_from_slice(&name);
    }
    fs::write(&gbk, bytes).unwrap();
    let report = library.import_paths(&[gbk]);
    assert_eq!(report.added, 1, "{report:?}");
    assert_eq!(
        library.sticker(&report.sticker_ids[0]).unwrap().name,
        "中文"
    );
    let utf8 = dir.path().join("utf8.zip");
    make_zip(&utf8, &[("路径/表情.png", &data)]);
    patch_zip_headers(&utf8, 6, 8, 0);
    assert_eq!(library.import_paths(&[utf8]).existing, 1);
}
#[test]
fn encrypted_unsupported_and_corrupt_archives_do_not_abort_other_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let image = dir.path().join("good.png");
    png_file(&image, [5, 6, 7, 255]);
    let data = fs::read(&image).unwrap();
    let encrypted = dir.path().join("加密.zip");
    make_zip(&encrypted, &[("子目录/a.png", &data)]);
    patch_zip_headers(&encrypted, 6, 8, 1);
    let unsupported = dir.path().join("不支持.zip");
    make_zip(&unsupported, &[("b.png", &data)]);
    patch_zip_headers(&unsupported, 8, 10, 12);
    let broken = dir.path().join("损坏.zip");
    fs::write(&broken, b"PKtruncated").unwrap();
    let report = library.import_paths(&[encrypted, unsupported, broken, image]);
    assert_eq!((report.added, report.failures.len()), (1, 3), "{report:?}");
    assert_eq!(report.failures[0].message, "不支持加密的压缩包");
    assert!(
        report.failures[0]
            .path
            .to_string_lossy()
            .ends_with("加密.zip › 子目录/a.png")
    );
    assert!(report.failures[1].message.contains("不支持的压缩方式"));
    assert!(report.failures[2].message.contains("压缩包损坏"));
    assert_eq!(
        fs::read_dir(library.root.join("objects")).unwrap().count(),
        1
    );
}

#[test]
fn corrupt_zip_entry_fails_alone_and_leaves_no_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let image = dir.path().join("other.png");
    png_file(&image, [3, 2, 1, 255]);
    let first = dir.path().join("first.png");
    png_file(&first, [4, 5, 6, 255]);
    let last = dir.path().join("last.png");
    png_file(&last, [7, 8, 9, 255]);
    let path = dir.path().join("broken.zip");
    make_zip(
        &path,
        &[
            ("first.png", &fs::read(&first).unwrap()),
            ("last.png", &fs::read(&last).unwrap()),
        ],
    );
    let mut bytes = fs::read(&path).unwrap();
    let headers: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter(|(_, w)| *w == b"PK\x01\x02")
        .map(|(i, _)| i)
        .collect();
    bytes[headers[1] + 16] ^= 1; // 最后一项 CRC 不匹配，目录与第一项仍然可读。
    fs::write(&path, bytes).unwrap();
    let report = library.import_paths(&[path, image]);
    assert_eq!(
        (report.added, report.existing, report.failures.len()),
        (2, 0, 1),
        "{report:?}"
    );
    assert!(report.failures[0].message.contains("条目损坏"));
    assert!(
        report.failures[0]
            .path
            .to_string_lossy()
            .ends_with("broken.zip › last.png")
    );
    let names: Vec<_> = report
        .sticker_ids
        .iter()
        .map(|id| library.sticker(id).unwrap().name)
        .collect();
    assert_eq!(names, ["first", "other"]);
    assert_eq!(
        fs::read_dir(library.root.join("objects")).unwrap().count(),
        2
    );
}
#[test]
fn cancel_keeps_committed_files_and_import_releases_lock_between_images() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let dir = tempfile::tempdir().unwrap();
    let library = Arc::new(Mutex::new(
        Library::open(&dir.path().join("library")).unwrap(),
    ));
    let paths: Vec<_> = (0..4)
        .map(|n| {
            let p = dir.path().join(format!("{n}.png"));
            png_file(&p, [n, 2, 3, 255]);
            p
        })
        .collect();
    let cancelled = AtomicBool::new(false);
    let report = import_paths_with_progress(
        &library,
        &paths,
        &|| cancelled.load(Ordering::Relaxed),
        &mut |p| {
            let other = library.clone();
            std::thread::spawn(move || assert!(other.try_lock().is_ok()))
                .join()
                .unwrap();
            if p.processed == 2 {
                cancelled.store(true, Ordering::Relaxed);
            }
        },
    )
    .unwrap();
    assert!(report.cancelled);
    assert_eq!((report.added, report.sticker_ids.len()), (2, 2));
    assert_eq!(library.lock().unwrap().overview().unwrap().all, 2);
}

#[test]
fn folder_import_puts_new_stickers_in_category_named_after_their_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let existing_category = library.create_category("Cats", None).unwrap();
    // 已在库里、没有分类的图，再从文件夹导入时保持原样
    let loose = dir.path().join("早就有.png");
    png_file(&loose, [9, 9, 9, 255]);
    library.import_paths(&[loose]);

    let root = dir.path().join("表情包");
    for folder in ["看戏", "cats", "看戏/更深", "未分类", "a..b"] {
        fs::create_dir_all(root.join(folder)).unwrap();
    }
    png_file(&root.join("散图.png"), [1, 0, 0, 255]);
    png_file(&root.join("看戏/吃瓜.png"), [2, 0, 0, 255]);
    png_file(&root.join("看戏/更深/搬凳子.png"), [3, 0, 0, 255]);
    png_file(&root.join("cats/猫.png"), [4, 0, 0, 255]);
    png_file(&root.join("未分类/没分类.png"), [5, 0, 0, 255]);
    png_file(&root.join("a..b/怪名字.png"), [6, 0, 0, 255]);
    png_file(&root.join("看戏/重复.png"), [9, 9, 9, 255]);
    let single = dir.path().join("单独选的.png");
    png_file(&single, [7, 0, 0, 255]);

    let report = library.import_paths(&[root, single]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!((report.added, report.existing), (7, 1));

    let stickers = library.list_stickers(LibraryView::All).unwrap();
    let category = |name: &str| {
        stickers
            .iter()
            .find(|s| s.name == name)
            .unwrap()
            .category
            .as_ref()
            .map(|c| c.name.clone())
    };
    assert_eq!(category("散图").as_deref(), Some("表情包"));
    assert_eq!(category("吃瓜").as_deref(), Some("看戏"));
    assert_eq!(category("搬凳子").as_deref(), Some("更深"));
    assert_eq!(category("猫").as_deref(), Some("Cats"));
    assert_eq!(category("没分类"), None);
    assert_eq!(category("怪名字"), None);
    assert_eq!(category("单独选的"), None);
    assert_eq!(category("早就有"), None);

    let mut names = library
        .categories()
        .unwrap()
        .into_iter()
        .map(|c| c.category.name)
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["Cats", "更深", "看戏", "表情包"]);
    assert!(
        library
            .categories()
            .unwrap()
            .iter()
            .any(|c| c.category.id == existing_category.id)
    );
}
