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
         INSERT INTO tags (id, name, color, position) VALUES ('t', '开心', '#F2B705', 0);
         INSERT INTO sticker_tags SELECT id, 't' FROM stickers;
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
            overview.untagged,
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
