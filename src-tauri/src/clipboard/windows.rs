use super::ClipboardRead;
use crate::windows_drop::{self, Source};
use std::{path::PathBuf, time::Duration};
use windows::Win32::{
    Foundation::CLIPBRD_E_CANT_OPEN,
    System::Ole::{OleGetClipboard, OleInitialize, OleUninitialize},
};

struct OleApartment;
impl Drop for OleApartment {
    fn drop(&mut self) {
        unsafe { OleUninitialize() };
    }
}
pub async fn read(root: PathBuf) -> Result<ClipboardRead, String> {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("sticker-clipboard".into())
        .spawn(move || {
            let result = (|| {
                unsafe { OleInitialize(None) }.map_err(|e| e.to_string())?;
                let _apartment = OleApartment;
                for attempt in 0..10 {
                    match unsafe { OleGetClipboard() } {
                        Ok(data) => {
                            windows_drop::diagnose_formats(&data, "clipboard");
                            return windows_drop::receive(&data, Source::Clipboard, &root);
                        }
                        Err(error) if error.code() == CLIPBRD_E_CANT_OPEN => {
                            if attempt < 9 {
                                std::thread::sleep(Duration::from_millis(30));
                            }
                        }
                        Err(error) => return Err(error.to_string()),
                    }
                }
                Err("剪贴板正被其他程序占用，请稍后再试".into())
            })();
            let _ = send.send(result);
        })
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || receive.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?
}
