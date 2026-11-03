use std::path::Path;

use image::DynamicImage;

pub type ClipboardResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// macOS 只写文件 URL，交出原文件，保留格式、透明通道和动画；`image` 只有 Windows 用。
#[cfg(target_os = "macos")]
pub fn write(path: &Path, _image: Option<&DynamicImage>) -> ClipboardResult<()> {
    use objc2::{rc::autoreleasepool, runtime::ProtocolObject};
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeFileURL};
    use objc2_foundation::{NSArray, NSString, NSURL};

    autoreleasepool(|_| {
        let path = path.to_str().ok_or("文件路径无法转换为文字")?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let url = url.absoluteString().ok_or("无法创建文件 URL")?;
        let item = NSPasteboardItem::new();
        if !item.setString_forType(&url, unsafe { NSPasteboardTypeFileURL }) {
            return Err("无法写入文件引用".into());
        }
        let items = NSArray::from_retained_slice(&[ProtocolObject::from_retained(item)]);
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        if !pasteboard.writeObjects(&items) {
            return Err("剪贴板写入失败".into());
        }
        Ok(())
    })
}

#[cfg(target_os = "windows")]
pub fn write(path: &Path, image: Option<&DynamicImage>) -> ClipboardResult<()> {
    use std::io::Cursor;

    use clipboard_win::{Clipboard, empty, formats, options, raw, register_format};

    let path = path.to_str().ok_or("文件路径无法转换为文字")?;
    // 在打开剪贴板之前完成编码，减少占用系统剪贴板的时间。
    let encoded = if let Some(image) = image {
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png)?;
        let mut bmp = Cursor::new(Vec::new());
        image.write_to(&mut bmp, image::ImageFormat::Bmp)?;
        let png_format = register_format("PNG").ok_or("无法注册 PNG 剪贴板格式")?;
        Some((png.into_inner(), bmp.into_inner(), png_format.get()))
    } else {
        None
    };
    let _clipboard = Clipboard::new_attempts(10).map_err(|error| error.to_string())?;
    empty().map_err(|error| error.to_string())?;
    let result = (|| -> ClipboardResult<()> {
        if let Some((png, bmp, png_format)) = encoded {
            raw::set_without_clear(png_format, &png).map_err(|error| error.to_string())?;
            // CF_DIB 不包含 BMP 的 14 字节文件头；同时提供 PNG 以保留透明度。
            raw::set_without_clear(formats::CF_DIB, &bmp[14..])
                .map_err(|error| error.to_string())?;
        }
        raw::set_file_list_with(&[path], options::NoClear).map_err(|error| error.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = empty();
    }
    result
}
