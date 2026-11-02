//! 托盘和悬浮球菜单共享原生动作，前端只请求打开固定菜单。
use crate::{
    ball, panel,
    settings::{self, SettingsState},
};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    AppHandle, Emitter, Manager,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

pub struct TrayState {
    ball_item: CheckMenuItem<tauri::Wry>,
    ball_menu: Menu<tauri::Wry>,
    preferences_pending: AtomicBool,
    sync_pending: AtomicBool,
    sync_item: MenuItem<tauri::Wry>,
}

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle();
    let panel_item = MenuItem::with_id(handle, "entry-panel", "显示表情面板", true, None::<&str>)?;
    let main = MenuItem::with_id(handle, "entry-main", "打开主窗口", true, None::<&str>)?;
    let ball_item = CheckMenuItem::with_id(
        handle,
        "entry-ball",
        "显示悬浮球",
        true,
        app.state::<SettingsState>().get().ball_visible,
        None::<&str>,
    )?;
    let preferences =
        MenuItem::with_id(handle, "entry-preferences", "偏好设置…", true, None::<&str>)?;
    let hide = MenuItem::with_id(handle, "entry-hide-ball", "隐藏悬浮球", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(handle)?;
    let config = app.state::<SettingsState>().get().sync;
    let sync_item = MenuItem::with_id(
        handle,
        "entry-sync-pause",
        if config.as_ref().is_some_and(|s| s.paused) {
            "继续同步"
        } else {
            "暂停同步"
        },
        config.is_some(),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(handle, "entry-quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        handle,
        &[
            &panel_item,
            &main,
            &ball_item,
            &preferences,
            &sync_item,
            &separator,
            &quit,
        ],
    )?;
    let ball_menu = Menu::with_items(
        handle,
        &[
            &main,
            &preferences,
            &hide,
            &sync_item,
            &PredefinedMenuItem::separator(handle)?,
            &quit,
        ],
    )?;
    #[cfg(target_os = "macos")]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template@2x.png"))?;
    #[cfg(not(target_os = "macos"))]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
    let builder = TrayIconBuilder::with_id("sticker-tray")
        .icon(icon)
        .tooltip("表情管理器")
        .icon_as_template(cfg!(target_os = "macos"))
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"));
    #[cfg(target_os = "windows")]
    let builder = builder.on_tray_icon_event(|tray, event| {
        if matches!(
            event,
            tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            }
        ) {
            dispatch(tray.app_handle(), "entry-main");
        }
    });
    builder.build(app)?;
    app.manage(TrayState {
        ball_item,
        ball_menu,
        preferences_pending: AtomicBool::new(false),
        sync_pending: AtomicBool::new(false),
        sync_item,
    });
    app.on_menu_event(|app, event| dispatch(app, event.id.as_ref()));
    Ok(())
}
pub fn set_ball_checked(app: &AppHandle, visible: bool) -> Result<(), String> {
    if let Some(tray) = app.try_state::<TrayState>() {
        tray.ball_item
            .set_checked(visible)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn dispatch(app: &AppHandle, id: &str) {
    if !id.starts_with("entry-") {
        return;
    }
    let id = id.to_string();
    let handle = app.clone();
    if id == "entry-sync-pause" {
        // 暂停要等仓库锁（同步可能正在写库），放到后台线程，菜单和界面不卡。
        tauri::async_runtime::spawn_blocking(move || {
            let paused = handle
                .state::<SettingsState>()
                .get()
                .sync
                .is_some_and(|s| s.paused);
            if let Err(error) = crate::sync::apply_paused(&handle, !paused) {
                eprintln!("菜单操作失败：{error}");
            }
        });
        return;
    }
    let result = app.run_on_main_thread(move || {
        let result = match id.as_str() {
            "entry-panel" => panel::toggle(&handle),
            "entry-main" => panel::show_main_window(handle.clone()),
            "entry-preferences" => open_preferences(&handle),
            "entry-ball" => settings::set_ball_visible(
                handle.clone(),
                !handle.state::<SettingsState>().get().ball_visible,
            )
            .map(|_| ()),
            "entry-hide-ball" => settings::set_ball_visible(handle.clone(), false).map(|_| ()),
            "entry-quit" => {
                handle.exit(0);
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            // 原生勾选项会先自行翻转；保存失败时恢复 Rust 中的实际值。
            if let Err(restore_error) =
                set_ball_checked(&handle, handle.state::<SettingsState>().get().ball_visible)
            {
                eprintln!("恢复托盘勾选失败：{restore_error}");
            }
            eprintln!("菜单操作失败：{error}");
        }
    });
    if let Err(error) = result {
        eprintln!("菜单操作失败：{error}");
    }
}
pub fn sync_menu_changed(app: &AppHandle) {
    if let Some(tray) = app.try_state::<TrayState>() {
        let config = app.state::<SettingsState>().get().sync;
        let _ = tray.sync_item.set_enabled(config.is_some());
        let _ = tray
            .sync_item
            .set_text(if config.is_some_and(|s| s.paused) {
                "继续同步"
            } else {
                "暂停同步"
            });
    }
}
#[tauri::command]
pub fn show_sync_details(app: AppHandle) -> Result<(), String> {
    app.state::<TrayState>()
        .sync_pending
        .store(true, Ordering::Relaxed);
    panel::show_main_window(app.clone())?;
    app.emit_to("main", "open-sync-details", ())
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn take_sync_details_request(app: AppHandle) -> bool {
    app.state::<TrayState>()
        .sync_pending
        .swap(false, Ordering::Relaxed)
}
fn open_preferences(app: &AppHandle) -> Result<(), String> {
    panel::show_main_window(app.clone())?;
    app.state::<TrayState>()
        .preferences_pending
        .store(true, Ordering::Relaxed);
    app.emit_to("main", "open-preferences", ())
        .map_err(|e| e.to_string())
}
// 监听后再取 pending，覆盖主窗口尚未完成加载时从菜单打开的情况。
#[tauri::command]
pub fn take_preferences_request(app: AppHandle) -> bool {
    app.state::<TrayState>()
        .preferences_pending
        .swap(false, Ordering::Relaxed)
}
#[tauri::command]
pub fn show_ball_menu(app: AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("ball").ok_or("悬浮球不可用")?;
    ball::menu_open(&app, true);
    let result = window
        .popup_menu(&app.state::<TrayState>().ball_menu)
        .map_err(|e| e.to_string());
    ball::menu_open(&app, false);
    result
}
