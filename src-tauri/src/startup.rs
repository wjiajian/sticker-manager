use std::path::{Path, PathBuf};

use std::sync::Mutex;
use tauri::{AppHandle, Manager};
#[cfg(not(target_os = "macos"))]
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};
use tauri_plugin_opener::OpenerExt;

pub struct StartupFailure(pub Mutex<Option<(PathBuf, String)>>);

pub fn show_pending(app: &AppHandle) {
    let Some(failure) = app.try_state::<StartupFailure>() else {
        return;
    };
    let pending = failure.0.lock().unwrap().take();
    if let Some((path, reason)) = pending {
        show_error(app, path, reason);
    }
}

pub fn reason(error: sticker_core::Error) -> String {
    match error {
        sticker_core::Error::SchemaTooNew { .. } => {
            "这个仓库由更新版本的应用创建，请更新应用后再打开".into()
        }
        error => error.to_string(),
    }
}

fn existing_directory(path: &Path) -> Option<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    absolute
        .ancestors()
        .find(|path| path.is_dir())
        .map(Path::to_path_buf)
}

fn finish(app: &AppHandle, path: &Path, reveal: bool) {
    if reveal
        && let Some(directory) = existing_directory(path)
        && let Err(error) = app.opener().reveal_item_in_dir(directory)
    {
        eprintln!("无法显示仓库位置：{error}");
    }
    app.exit(1);
}

#[cfg(target_os = "macos")]
fn show_error(app: &AppHandle, path: PathBuf, reason: String) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAlert, NSAlertSecondButtonReturn, NSAlertStyle, NSApplication};
    use objc2_foundation::NSString;

    let mtm = MainThreadMarker::new().expect("启动警告在主线程显示");
    let alert = NSAlert::new(mtm);
    alert.setAlertStyle(NSAlertStyle::Warning);
    alert.setMessageText(&NSString::from_str("无法打开表情仓库"));
    alert.setInformativeText(&NSString::from_str(&format!(
        "仓库路径：{}\n\n{reason}",
        path.display()
    )));
    alert
        .addButtonWithTitle(&NSString::from_str("退出"))
        .setKeyEquivalent(&NSString::from_str("\r"));
    alert.addButtonWithTitle(&NSString::from_str("在访达中显示"));
    NSApplication::sharedApplication(mtm).activate();
    // NSAlert 自己运行模态事件循环；插件的 blocking_show 不能在主线程使用。
    finish(app, &path, alert.runModal() == NSAlertSecondButtonReturn);
}

#[cfg(not(target_os = "macos"))]
fn show_error(app: &AppHandle, path: PathBuf, reason: String) {
    let reveal = if cfg!(target_os = "windows") {
        "在资源管理器中显示"
    } else {
        "在访达中显示"
    };
    let handle = app.clone();
    app.dialog()
        .message(format!("仓库路径：{}\n\n{reason}", path.display()))
        .title("无法打开表情仓库")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "退出".into(),
            reveal.into(),
        ))
        .show_with_result(move |result| {
            finish(
                &handle,
                &path,
                result == MessageDialogResult::Custom(reveal.into()),
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_uses_existing_directory_for_files_and_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ordinary-file");
        std::fs::write(&file, "not a library").unwrap();
        assert_eq!(existing_directory(&file), Some(dir.path().to_path_buf()));
        assert_eq!(
            existing_directory(&dir.path().join("missing/nested")),
            Some(dir.path().to_path_buf())
        );
    }
}
