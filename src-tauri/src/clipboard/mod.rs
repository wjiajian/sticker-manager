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

pub(crate) mod html;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "windows")]
mod windows;

use crate::staging::{Staged, StagingRoot};
use std::sync::atomic::{AtomicBool, Ordering};
use sticker_core::ImportReport;
use tauri::{AppHandle, Emitter, Manager};

#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ClipboardImport {
    Imported { report: ImportReport },
    Empty { message: String },
}
pub enum ClipboardRead {
    Staged(Staged),
    Empty { remote_only: bool },
}
static READING: AtomicBool = AtomicBool::new(false);
struct Reading;
impl Drop for Reading {
    fn drop(&mut self) {
        READING.store(false, Ordering::Release);
    }
}
async fn import(app: AppHandle, ball: bool) -> Result<ClipboardImport, String> {
    const BUSY: &str = "正在导入另一批表情，请等它完成";
    if READING.swap(true, Ordering::AcqRel) {
        return Err(BUSY.into());
    }
    let _guard = Reading;
    if crate::imports::status(&app).running {
        return Err(BUSY.into());
    }
    let root = app.state::<StagingRoot>().0.clone();
    #[cfg(target_os = "macos")]
    let read = tauri::async_runtime::spawn_blocking(move || macos::read(&root))
        .await
        .map_err(|e| e.to_string())??;
    #[cfg(target_os = "windows")]
    let read = windows::read(root).await?;
    match read {
        ClipboardRead::Staged(staged) => Ok(ClipboardImport::Imported {
            report: crate::staging::import(app, staged, ball).await?,
        }),
        ClipboardRead::Empty { remote_only } => Ok(ClipboardImport::Empty {
            message: if remote_only {
                "剪贴板里只有网络图片地址，请在浏览器中复制图片本身，或先保存后导入"
            } else {
                "剪贴板里没有可导入的图片或文件"
            }
            .into(),
        }),
    }
}
#[tauri::command]
pub async fn import_clipboard(app: AppHandle) -> Result<ClipboardImport, String> {
    import(app, false).await
}
pub fn import_from_ball_menu(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match import(app.clone(), true).await {
            Ok(ClipboardImport::Imported { report }) => {
                let _ = app.emit_to("ball", "external-drop-result", report);
            }
            Ok(ClipboardImport::Empty { message }) | Err(message) => {
                crate::ball::notify(&app, message)
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_import_serializes_with_kind_tag() {
        let empty = serde_json::to_value(ClipboardImport::Empty {
            message: "空".into(),
        })
        .unwrap();
        assert_eq!(
            empty,
            serde_json::json!({ "kind": "empty", "message": "空" })
        );

        let imported = serde_json::to_value(ClipboardImport::Imported {
            report: ImportReport {
                added: 2,
                skipped: 1,
                ..ImportReport::default()
            },
        })
        .unwrap();
        assert_eq!(imported["kind"], "imported");
        assert_eq!(imported["report"]["added"], 2);
        assert_eq!(imported["report"]["skipped"], 1);
    }

    #[test]
    fn reading_guard_prevents_reentry_and_releases_on_drop() {
        assert!(!READING.swap(true, Ordering::AcqRel));
        {
            let _guard = Reading;
            assert!(READING.swap(true, Ordering::AcqRel));
        }
        assert!(!READING.load(Ordering::Acquire));
    }
}
