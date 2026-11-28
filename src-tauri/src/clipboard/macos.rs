use super::{ClipboardRead, html};
use crate::{
    diagnostics::drop_diag,
    staging::{self, MAX_BYTES, MAX_ITEMS, Origin, Staged, Staging},
};
use objc2::{
    ClassType, exception,
    rc::{Retained, autoreleasepool},
    runtime::AnyObject,
};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardURLReadingFileURLsOnlyKey,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSNumber, NSString, NSURL};
use std::path::{Path, PathBuf};
use sticker_core::ImportFailure;

pub const IMAGE_UTIS: &[&str] = &[
    "com.compuserve.gif",
    "org.webmproject.webp",
    "public.webp",
    "public.png",
    "public.jpeg",
    "public.tiff",
];
pub fn types(names: &[&str]) -> Retained<NSArray<NSString>> {
    NSArray::from_retained_slice(
        &names
            .iter()
            .map(|name| NSString::from_str(name))
            .collect::<Vec<_>>(),
    )
}
pub struct FileScan {
    pub readable: Vec<PathBuf>,
    pub unreadable: bool,
    pub failures: Vec<ImportFailure>,
}
enum FileAccess {
    Readable,
    Unreadable,
    Missing,
}
fn access(path: &Path, check_readability: bool) -> FileAccess {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return FileAccess::Unreadable;
        }
        Err(_) => return FileAccess::Missing,
    };
    if !(meta.is_file() || meta.is_dir()) {
        // Never open non-regular files (FIFOs, sockets, devices).
        return FileAccess::Missing;
    }
    if !check_readability {
        return FileAccess::Readable;
    }
    let probe = if meta.is_dir() {
        std::fs::read_dir(path).map(|_| ())
    } else {
        std::fs::File::open(path).map(|_| ())
    };
    match probe {
        Ok(()) => FileAccess::Readable,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => FileAccess::Unreadable,
        Err(_) => FileAccess::Missing,
    }
}
pub fn files(board: &NSPasteboard) -> FileScan {
    scan_files(board, true)
}
pub fn probe_files(board: &NSPasteboard, check_readability: bool) -> FileScan {
    scan_files(board, check_readability)
}
fn scan_files(board: &NSPasteboard, check_readability: bool) -> FileScan {
    let mut readable = vec![];
    let mut unreadable = false;
    let mut failures = vec![];
    let mut seen = std::collections::HashSet::new();
    let mut overflow = 0usize;
    let mut add = |path: PathBuf| {
        if !seen.insert(path.clone()) {
            return;
        }
        if seen.len() > MAX_ITEMS {
            overflow += 1;
            return;
        }
        match access(&path, check_readability) {
            FileAccess::Readable => readable.push(path),
            FileAccess::Unreadable => {
                unreadable = true;
                failures.push(ImportFailure {
                    path,
                    message: "没有权限读取该文件，请先另存图片或在系统设置中授权后导入".into(),
                    count: 1,
                });
            }
            FileAccess::Missing => {
                failures.push(ImportFailure {
                    path,
                    message: "文件不存在".into(),
                    count: 1,
                });
            }
        }
    };
    let number = NSNumber::numberWithBool(true);
    let options = NSDictionary::<NSString, AnyObject>::from_slices(
        &[unsafe { NSPasteboardURLReadingFileURLsOnlyKey }],
        &[&number],
    );
    let classes = NSArray::from_slice(&[NSURL::class()]);
    if let Some(items) = unsafe { board.readObjectsForClasses_options(&classes, Some(&options)) } {
        for item in items.iter() {
            if let Some(url) = item.downcast_ref::<NSURL>()
                && url.isFileURL()
                && url
                    .host()
                    .is_none_or(|h| matches!(h.to_string().as_str(), "" | "localhost"))
                && let Some(path) = url.filePathURL().and_then(|url| url.path())
            {
                add(PathBuf::from(path.to_string()));
            }
        }
    }
    // A missing file URL may not be materialized by readObjectsForClasses.
    if let Some(items) = board.pasteboardItems() {
        for item in items.iter() {
            if let Some(value) = item.stringForType(&NSString::from_str("public.file-url"))
                && let Some(path) = html::local_file(&value.to_string())
                && !path.to_string_lossy().starts_with("/.file/id=")
            {
                add(path);
            }
        }
    }
    if overflow > 0 {
        failures.push(ImportFailure {
            path: "文件".into(),
            message: "图片超过 256 项".into(),
            count: overflow,
        });
    }
    FileScan {
        readable,
        unreadable,
        failures,
    }
}
pub struct Raw {
    data: Vec<u8>,
    tiff: bool,
}
impl Raw {
    pub fn animated_format(&self) -> bool {
        matches!(staging::sniff(&self.data), Some("gif" | "webp"))
    }
}
pub struct RawBatch {
    items: Vec<Raw>,
    failures: Vec<ImportFailure>,
}
impl RawBatch {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn has_animation(&self) -> bool {
        self.items.iter().any(Raw::animated_format)
    }
    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }
}
pub fn read_raw(board: &NSPasteboard, utis: &[&str]) -> RawBatch {
    let mut result = RawBatch {
        items: vec![],
        failures: vec![],
    };
    let mut total = 0;
    let mut read = |get: &dyn Fn(&NSString) -> Option<Retained<NSData>>| {
        let mut has_data = false;
        let mut over_limit = false;
        for &uti in utis {
            if let Some(data) = get(&NSString::from_str(uti)) {
                let len = data.length();
                if len == 0 {
                    continue;
                }
                has_data = true;
                if len > MAX_BYTES - total {
                    over_limit = true;
                    continue;
                }
                let data = data.to_vec();
                if staging::sniff(&data).is_some() || uti == "public.tiff" {
                    total += len;
                    result.items.push(Raw {
                        data,
                        tiff: uti == "public.tiff",
                    });
                    return;
                }
            }
        }
        if has_data {
            result.failures.push(ImportFailure {
                path: "图片".into(),
                message: if over_limit {
                    "图片数据总量超过 100 MB"
                } else {
                    "无法识别图片格式"
                }
                .into(),
                count: 1,
            });
        }
    };
    if let Some(items) = board.pasteboardItems() {
        for item in items.iter().take(MAX_ITEMS) {
            read(&|uti| item.dataForType(uti));
        }
        if items.len() > MAX_ITEMS {
            result.failures.push(ImportFailure {
                path: "图片".into(),
                message: "图片超过 256 项".into(),
                count: items.len() - MAX_ITEMS,
            });
        }
    } else {
        read(&|uti| board.dataForType(uti));
    }
    result
}
pub fn tiff_png(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    autoreleasepool(|_| {
        exception::catch(std::panic::AssertUnwindSafe(|| {
            let image = NSBitmapImageRep::imageRepWithData(&NSData::from_vec(bytes))
                .ok_or("无法解码 TIFF 图片")?;
            let data = unsafe {
                image.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
            }
            .ok_or("无法转换 TIFF 图片")?;
            if data.length() > MAX_BYTES {
                return Err("图片数据超过 100 MB");
            }
            Ok(data.to_vec())
        }))
        .map_err(|_| "无法转换 TIFF 图片".to_string())?
        .map_err(str::to_owned)
    })
}
pub fn stage_raw(staging: &mut Staging, raw: RawBatch) {
    for failure in raw.failures {
        staging.fail("图片", failure.message, failure.count);
    }
    for item in raw.items {
        let result = if item.tiff && staging::sniff(&item.data).is_none() {
            tiff_png(item.data)
        } else {
            Ok(item.data)
        };
        if let Err(error) = result.and_then(|data| staging.add_bytes(&data)) {
            staging.fail("图片", error, 1);
        }
    }
}
fn read_item_raw(
    get: &dyn Fn(&NSString) -> Option<Retained<NSData>>,
    utis: &[&str],
    total: &mut usize,
) -> Option<Result<Raw, &'static str>> {
    let mut has_data = false;
    let mut over_limit = false;
    for &uti in utis {
        if let Some(data) = get(&NSString::from_str(uti)) {
            let len = data.length();
            if len == 0 {
                continue;
            }
            has_data = true;
            if len > MAX_BYTES - *total {
                over_limit = true;
                continue;
            }
            let data = data.to_vec();
            if staging::sniff(&data).is_some() || uti == "public.tiff" {
                *total += len;
                return Some(Ok(Raw {
                    data,
                    tiff: uti == "public.tiff",
                }));
            }
        }
    }
    if has_data {
        Some(Err(if over_limit {
            "图片数据总量超过 100 MB"
        } else {
            "无法识别图片格式"
        }))
    } else {
        None
    }
}
fn stage_one(staging: &mut Staging, item: Raw) {
    let result = if item.tiff && staging::sniff(&item.data).is_none() {
        tiff_png(item.data)
    } else {
        Ok(item.data)
    };
    if let Err(error) = result.and_then(|data| staging.add_bytes(&data)) {
        staging.fail("图片", error, 1);
    }
}
fn read_board(board: &NSPasteboard, root: &Path) -> Result<ClipboardRead, String> {
    let scan = files(board);
    if !scan.readable.is_empty() {
        let mut staged = Staged::existing(scan.readable);
        staged.failures = scan.failures;
        return Ok(ClipboardRead::Staged(staged));
    }
    let mut s = Staging::new(root, Origin::Clipboard)?;
    let mut remote_only = false;
    let mut total = 0usize;
    let mut process_entry =
        |get_data: &dyn Fn(&NSString) -> Option<Retained<NSData>>,
         get_html: &dyn Fn() -> Option<Retained<NSString>>| {
            // Per-item priority: animated bytes -> local/data HTML images -> static image bytes.
            match read_item_raw(get_data, &IMAGE_UTIS[..3], &mut total) {
                Some(Ok(raw)) => {
                    stage_one(&mut s, raw);
                    return;
                }
                Some(Err(msg)) => s.fail("图片", msg, 1),
                None => {}
            }
            if let Some(value) = get_html() {
                let images = html::images(&value.to_string(), &mut s);
                if images.remote_only {
                    remote_only = true;
                }
                if images.added > 0 {
                    return;
                }
            }
            match read_item_raw(get_data, &IMAGE_UTIS[3..], &mut total) {
                Some(Ok(raw)) => stage_one(&mut s, raw),
                Some(Err(msg)) => s.fail("图片", msg, 1),
                None => {}
            }
        };
    if let Some(items) = board.pasteboardItems() {
        for item in items.iter().take(MAX_ITEMS) {
            process_entry(&|uti| item.dataForType(uti), &|| {
                item.stringForType(&NSString::from_str("public.html"))
            });
        }
        if items.len() > MAX_ITEMS {
            s.fail("图片", "图片超过 256 项", items.len() - MAX_ITEMS);
        }
    } else {
        process_entry(&|uti| board.dataForType(uti), &|| {
            board.stringForType(&NSString::from_str("public.html"))
        });
    }
    let staged = s.finish();
    if !staged.paths.is_empty() {
        return Ok(ClipboardRead::Staged(staged));
    }
    if !scan.failures.is_empty() {
        let mut missing = Staged::existing(vec![]);
        missing.failures = scan.failures;
        return Ok(ClipboardRead::Staged(missing));
    }
    if !staged.failures.is_empty() {
        return Ok(ClipboardRead::Staged(staged));
    }
    Ok(ClipboardRead::Empty { remote_only })
}
pub fn read(root: &Path) -> Result<ClipboardRead, String> {
    autoreleasepool(|_| {
        exception::catch(std::panic::AssertUnwindSafe(|| {
            let board = NSPasteboard::generalPasteboard();
            if crate::diagnostics::enabled()
                && let Some(types) = board.types()
            {
                for t in types.iter() {
                    drop_diag!("clipboard type={t}");
                }
            }
            read_board(&board, root)
        }))
        .map_err(|_| "无法读取剪贴板".to_string())?
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::NSPasteboardItem;

    struct UniqueBoard(Retained<NSPasteboard>);
    impl Drop for UniqueBoard {
        fn drop(&mut self) {
            self.0.clearContents();
        }
    }
    fn board_items(items: Vec<Retained<NSPasteboardItem>>) -> UniqueBoard {
        let board = NSPasteboard::pasteboardWithUniqueName();
        board.clearContents();
        let items = NSArray::from_retained_slice(
            &items
                .into_iter()
                .map(ProtocolObject::from_retained)
                .collect::<Vec<_>>(),
        );
        assert!(board.writeObjects(&items));
        UniqueBoard(board)
    }
    fn image(uti: &str, bytes: &[u8]) -> Retained<NSPasteboardItem> {
        let item = NSPasteboardItem::new();
        assert!(item.setData_forType(&NSData::from_vec(bytes.to_vec()), &NSString::from_str(uti)));
        item
    }
    fn file(path: &Path) -> Retained<NSPasteboardItem> {
        let item = NSPasteboardItem::new();
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str().unwrap()));
        assert!(item.setString_forType(
            &url.absoluteString().unwrap(),
            &NSString::from_str("public.file-url")
        ));
        item
    }
    fn sample_tiff(rgba: [u8; 4]) -> Vec<u8> {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(2, 3, image::Rgba(rgba)))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let rep = NSBitmapImageRep::imageRepWithData(&NSData::from_vec(png.into_inner())).unwrap();
        let tiff = unsafe {
            rep.representationUsingType_properties(
                NSBitmapImageFileType::TIFF,
                &NSDictionary::new(),
            )
        }
        .unwrap();
        tiff.to_vec()
    }
    #[test]
    fn named_pasteboard_preserves_gif_over_png_and_html() {
        let root = tempfile::tempdir().unwrap();
        let item = image("public.png", b"\x89PNG\r\n\x1a\n");
        assert!(item.setData_forType(
            &NSData::from_vec(b"GIF89a".to_vec()),
            &NSString::from_str("com.compuserve.gif")
        ));
        assert!(item.setString_forType(
            &NSString::from_str("<img src=data:image/webp;base64,UklGRi4uLi5XRUJQ>"),
            &NSString::from_str("public.html")
        ));
        let board = board_items(vec![item]);
        let ClipboardRead::Staged(staged) = read_board(&board.0, root.path()).unwrap() else {
            panic!("expected GIF")
        };
        assert_eq!(std::fs::read(&staged.paths[0]).unwrap(), b"GIF89a");
    }
    #[test]
    fn named_pasteboard_tracks_missing_files_and_falls_back() {
        let root = tempfile::tempdir().unwrap();
        let valid = root.path().join("present.gif");
        std::fs::write(&valid, b"GIF89a").unwrap();
        let missing = root.path().join("missing.gif");
        let board = board_items(vec![file(&valid), file(&missing)]);
        let ClipboardRead::Staged(staged) = read_board(&board.0, root.path()).unwrap() else {
            panic!("expected file")
        };
        assert_eq!(staged.paths, [valid]);
        assert_eq!(staged.failures.len(), 1);
        drop(board);
        let board = board_items(vec![file(&missing)]);
        let ClipboardRead::Staged(staged) = read_board(&board.0, root.path()).unwrap() else {
            panic!("expected missing file failure")
        };
        assert!(staged.paths.is_empty());
        assert_eq!(staged.failures.len(), 1);
        drop(board);
        let board = board_items(vec![
            file(&missing),
            image("public.png", b"\x89PNG\r\n\x1a\n"),
        ]);
        let ClipboardRead::Staged(staged) = read_board(&board.0, root.path()).unwrap() else {
            panic!("expected fallback")
        };
        assert_eq!(
            std::fs::read(&staged.paths[0]).unwrap(),
            b"\x89PNG\r\n\x1a\n"
        );
    }
    #[test]
    fn named_pasteboard_tier_priorities_and_empty_states() {
        let root = tempfile::tempdir().unwrap();
        let tiff = sample_tiff([30, 60, 90, 128]);

        // Tier 1: file URL + icon TIFF uses file tier.
        let file_path = root.path().join("finder.png");
        std::fs::write(&file_path, b"\x89PNG\r\n\x1a\n").unwrap();
        let item = file(&file_path);
        assert!(item.setData_forType(
            &NSData::from_vec(tiff.clone()),
            &NSString::from_str("public.tiff")
        ));
        let board = board_items(vec![item]);
        let ClipboardRead::Staged(staged) = read_board(&board.0, root.path()).unwrap() else {
            panic!("expected file tier")
        };
        assert_eq!(staged.paths, [file_path]);
        drop(board);

        // Tier 2: GIF + TIFF preserves GIF; two items (GIF + PNG) selects animated tier.
        let gif_tiff = image("public.tiff", &tiff);
        assert!(gif_tiff.setData_forType(
            &NSData::from_vec(b"GIF89a".to_vec()),
            &NSString::from_str("com.compuserve.gif")
        ));
        let board = board_items(vec![gif_tiff]);
        let ClipboardRead::Staged(staged) = read_board(&board.0, root.path()).unwrap() else {
            panic!("expected GIF over TIFF")
        };
        assert_eq!(std::fs::read(&staged.paths[0]).unwrap(), b"GIF89a");
        drop(board);

        // Two independent items (GIF + PNG): preserve both items with per-item best representation.
        let board = board_items(vec![
            image("com.compuserve.gif", b"GIF89a"),
            image("public.png", b"\x89PNG\r\n\x1a\n"),
        ]);
        let ClipboardRead::Staged(staged) = read_board(&board.0, root.path()).unwrap() else {
            panic!("expected both independent images")
        };
        assert_eq!(staged.paths.len(), 2);
        assert_eq!(std::fs::read(&staged.paths[0]).unwrap(), b"GIF89a");
        assert_eq!(
            std::fs::read(&staged.paths[1]).unwrap(),
            b"\x89PNG\r\n\x1a\n"
        );
        drop(board);

        // Single item with HTML data:GIF + static PNG picks HTML GIF; a second independent PNG
        // item on the same pasteboard is also preserved rather than dropped.
        let html_png_item = image("public.png", b"\x89PNG\r\n\x1a\n");
        assert!(html_png_item.setString_forType(
            &NSString::from_str("<img src=\"data:image/gif;base64,R0lGODlh\">"),
            &NSString::from_str("public.html")
        ));
        let board = board_items(vec![
            html_png_item,
            image("public.png", b"\x89PNG\r\n\x1a\n"),
        ]);
        let ClipboardRead::Staged(staged) = read_board(&board.0, root.path()).unwrap() else {
            panic!("expected HTML GIF and independent PNG")
        };
        assert_eq!(staged.paths.len(), 2);
        assert_eq!(std::fs::read(&staged.paths[0]).unwrap(), b"GIF89a");
        assert_eq!(
            std::fs::read(&staged.paths[1]).unwrap(),
            b"\x89PNG\r\n\x1a\n"
        );
        drop(board);

        // Decodable HTML base64 with unrecognized image bytes surfaces the staging failure
        // rather than reporting an empty clipboard.
        let bad_html_item = NSPasteboardItem::new();
        assert!(bad_html_item.setString_forType(
            &NSString::from_str("<img src=\"data:image/png;base64,bm90LWEtcG5n\">"),
            &NSString::from_str("public.html")
        ));
        let board = board_items(vec![bad_html_item]);
        let ClipboardRead::Staged(staged) = read_board(&board.0, root.path()).unwrap() else {
            panic!("expected staging failure to be surfaced")
        };
        assert!(staged.paths.is_empty());
        assert_eq!(staged.failures.len(), 1);
        drop(board);

        // Tier 4: TIFF only converts to PNG with alpha preserved.
        let board = board_items(vec![image("public.tiff", &tiff)]);
        let ClipboardRead::Staged(staged) = read_board(&board.0, root.path()).unwrap() else {
            panic!("expected TIFF conversion")
        };
        let decoded = image::open(&staged.paths[0]).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (2, 3));
        assert_eq!(decoded.get_pixel(0, 0).0, [30, 60, 90, 128]);
        drop(board);

        // Empty states: HTML remote-only vs plain text.
        let remote_html = NSPasteboardItem::new();
        assert!(remote_html.setString_forType(
            &NSString::from_str("<img src=\"https://example.com/a.png\">"),
            &NSString::from_str("public.html")
        ));
        let board = board_items(vec![remote_html]);
        assert!(matches!(
            read_board(&board.0, root.path()).unwrap(),
            ClipboardRead::Empty { remote_only: true }
        ));
        drop(board);

        let text = NSPasteboardItem::new();
        assert!(text.setString_forType(
            &NSString::from_str("hello sticker"),
            &NSString::from_str("public.utf8-plain-text")
        ));
        let board = board_items(vec![text]);
        assert!(matches!(
            read_board(&board.0, root.path()).unwrap(),
            ClipboardRead::Empty { remote_only: false }
        ));
    }
    #[test]
    fn unreadable_file_url_falls_back_to_tiff_bytes() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let locked = root.path().join("sandboxed.png");
        std::fs::write(&locked, b"\x89PNG\r\n\x1a\n").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let tiff = sample_tiff([12, 34, 56, 255]);
        let item = file(&locked);
        assert!(item.setData_forType(&NSData::from_vec(tiff), &NSString::from_str("public.tiff")));
        let board = board_items(vec![item]);
        let outcome = read_board(&board.0, root.path()).unwrap();
        let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644));
        let ClipboardRead::Staged(staged) = outcome else {
            panic!("expected TIFF fallback when file URL is unreadable")
        };
        assert_eq!(staged.paths.len(), 1);
        let decoded = image::open(&staged.paths[0]).unwrap().to_rgba8();
        assert_eq!(decoded.get_pixel(0, 0).0, [12, 34, 56, 255]);
    }
    #[test]
    fn tiff_conversion_preserves_dimensions_and_pixel() {
        let tiff = sample_tiff([30, 60, 90, 255]);
        let converted = tiff_png(tiff).unwrap();
        let image = image::load_from_memory(&converted).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (2, 3));
        assert_eq!(image.get_pixel(0, 0).0, [30, 60, 90, 255]);
        assert!(tiff_png(b"not a TIFF".to_vec()).is_err());
    }
}
