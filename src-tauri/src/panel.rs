//! 表情面板的窗口、全局快捷键和运行期间的固定状态。
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
#[cfg(any(not(target_os = "macos"), test))]
use tauri::PhysicalPosition;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const WIDTH: f64 = 360.0;
const HEIGHT: f64 = 524.0;

#[derive(Default)]
pub struct PanelState {
    pinned: AtomicBool,
    dragging: AtomicBool,
    shortcut_error: Mutex<Option<String>>,
}

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    app.manage(PanelState::default());
    native::create(app)?;
    Ok(())
}

pub fn register_shortcut(app: &AppHandle) {
    let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyE);
    let result = app
        .global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            // NSPanel 操作只能在 AppKit 主线程上执行。
            let handle = app.clone();
            if let Err(error) = app.run_on_main_thread(move || {
                if let Err(error) = toggle(&handle) {
                    eprintln!("无法切换表情面板：{error}");
                }
            }) {
                eprintln!("无法切换表情面板：{error}");
            }
        });
    if let Err(error) = result {
        let shortcut = if cfg!(target_os = "macos") {
            "⌃⌥E"
        } else {
            "Ctrl+Alt+E"
        };
        *app.state::<PanelState>().shortcut_error.lock().unwrap() =
            Some(format!("全局快捷键 {shortcut} 注册失败：{error}"));
    }
}

fn window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("panel")
        .ok_or_else(|| "表情面板不可用".into())
}

pub fn toggle(app: &AppHandle) -> Result<(), String> {
    let window = window(app)?;
    if native::visible(app)? {
        if native::focused(app)? {
            hide(app)?;
        } else {
            native::focus(app)?;
            window.emit("panel-focus", ()).map_err(|e| e.to_string())?;
        }
    } else {
        native::center_at_pointer(app)?;
        native::focus(app)?;
        window.emit("panel-shown", ()).map_err(|e| e.to_string())?;
    }
    broadcast_visibility(app)?;
    Ok(())
}

pub fn is_visible(app: &AppHandle) -> Result<bool, String> {
    native::visible(app)
}

fn broadcast_visibility(app: &AppHandle) -> Result<(), String> {
    app.emit("panel-visibility", native::visible(app)?)
        .map_err(|e| e.to_string())
}

fn hide(app: &AppHandle) -> Result<(), String> {
    native::hide(app)?;
    broadcast_visibility(app)
}

pub fn show_beside_ball(
    app: &AppHandle,
    display: &crate::geometry::Display,
    center: crate::geometry::Point,
    edge: crate::settings::Edge,
) -> Result<(), String> {
    let p = crate::geometry::beside_ball(display, center, edge, WIDTH, HEIGHT);
    crate::desktop::set_frame(
        app,
        "panel",
        crate::geometry::Rect {
            x: p.x,
            y: p.y,
            width: WIDTH * display.scale,
            height: HEIGHT * display.scale,
        },
    )?;
    native::focus(app)?;
    window(app)?
        .emit("panel-shown", ())
        .map_err(|e| e.to_string())?;
    broadcast_visibility(app)
}

#[cfg(any(not(target_os = "macos"), test))]
fn centered_position(
    origin: PhysicalPosition<i32>,
    size: tauri::PhysicalSize<u32>,
    scale: f64,
) -> PhysicalPosition<i32> {
    PhysicalPosition::new(
        origin.x + ((f64::from(size.width) - WIDTH * scale).max(0.0) / 2.0).round() as i32,
        origin.y + ((f64::from(size.height) - HEIGHT * scale).max(0.0) / 2.0).round() as i32,
    )
}

pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let result = if window.label() == "panel" {
            hide(window.app_handle())
        } else {
            window.hide().map_err(|e| e.to_string())
        };
        if let Err(error) = result {
            eprintln!("无法隐藏窗口：{error}");
        }
    }
    if window.label() == "panel" && matches!(event, WindowEvent::Focused(false)) {
        let state = window.state::<PanelState>();
        if !state.pinned.load(Ordering::Relaxed)
            && !state.dragging.load(Ordering::Relaxed)
            && let Err(error) = hide(window.app_handle())
        {
            eprintln!("无法收起表情面板：{error}");
        }
    }
}

#[derive(Serialize)]
pub struct PanelSession {
    pinned: bool,
    visible: bool,
}

#[tauri::command]
pub fn panel_session(app: AppHandle) -> Result<PanelSession, String> {
    Ok(PanelSession {
        pinned: app.state::<PanelState>().pinned.load(Ordering::Relaxed),
        visible: native::visible(&app)?,
    })
}

#[tauri::command]
pub fn set_panel_pinned(app: AppHandle, pinned: bool) {
    app.state::<PanelState>()
        .pinned
        .store(pinned, Ordering::Relaxed);
}

#[tauri::command]
pub fn set_panel_dragging(app: AppHandle, dragging: bool) -> Result<(), String> {
    let state = app.state::<PanelState>();
    state.dragging.store(dragging, Ordering::Relaxed);
    if !dragging && !state.pinned.load(Ordering::Relaxed) && !native::focused(&app)? {
        hide(&app)?;
    }
    Ok(())
}

/// 返回是否已收起。复制成功时由 Rust 判断固定状态，避免界面状态更新的竞态。
#[tauri::command]
pub fn hide_panel(app: AppHandle, only_if_unpinned: bool) -> Result<bool, String> {
    if only_if_unpinned && app.state::<PanelState>().pinned.load(Ordering::Relaxed) {
        return Ok(false);
    }
    hide(&app)?;
    Ok(true)
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) -> Result<(), String> {
    hide(&app)?;
    if let Some(window) = app.get_webview_window("main") {
        window.unminimize().map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn global_shortcut_error(state: tauri::State<PanelState>) -> Option<String> {
    state.shortcut_error.lock().unwrap().clone()
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use tauri_nspanel::{
        CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask, tauri_panel,
    };

    tauri_panel! {
        panel!(StickerPanel {
            config: {
                can_become_key_window: true,
                can_become_main_window: false,
                is_floating_panel: true
            }
        })
    }

    pub fn create(app: &tauri::App) -> tauri::Result<()> {
        PanelBuilder::<_, StickerPanel>::new(app.handle(), "panel")
            .url(tauri::WebviewUrl::App("panel.html".into()))
            .title("表情面板")
            .size(tauri::LogicalSize::new(WIDTH, HEIGHT).into())
            .resizable(false)
            .level(PanelLevel::Floating)
            .has_shadow(true)
            .corner_radius(24.0)
            .transparent(true)
            .hides_on_deactivate(false)
            .becomes_key_only_if_needed(false)
            .add_style_mask(StyleMask::empty().nonactivating_panel())
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary()
                    .ignores_cycle(),
            )
            .with_window(|w| {
                w.decorations(false)
                    .visible(false)
                    .focused(false)
                    .accept_first_mouse(true)
                    .skip_taskbar(true)
                    .minimizable(false)
                    .maximizable(false)
            })
            .build()?;
        Ok(())
    }

    pub fn center_at_pointer(app: &AppHandle) -> Result<(), String> {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSEvent, NSScreen};
        use objc2_foundation::NSPoint;

        let mtm = MainThreadMarker::new().ok_or("窗口操作必须在主线程进行")?;
        // AppKit 的鼠标位置、屏幕 frame 和 visibleFrame 都是左下原点的逻辑坐标。
        // 不与 Tauri 的物理鼠标坐标混用，也不依赖面板原来所在屏幕的缩放比例。
        let cursor = NSEvent::mouseLocation();
        let screens = NSScreen::screens(mtm);
        let screen = screens
            .iter()
            .find(|screen| {
                let frame = screen.frame();
                cursor.x >= frame.origin.x
                    && cursor.x < frame.origin.x + frame.size.width
                    && cursor.y >= frame.origin.y
                    && cursor.y < frame.origin.y + frame.size.height
            })
            .ok_or("无法确定鼠标所在的屏幕")?;
        let area = screen.visibleFrame();
        let position = NSPoint::new(
            area.origin.x + ((area.size.width - WIDTH).max(0.0) / 2.0).round(),
            area.origin.y + ((area.size.height - HEIGHT).max(0.0) / 2.0).round(),
        );
        app.get_webview_panel("panel")
            .map_err(|e| format!("{e:?}"))?
            .as_panel()
            .setFrameOrigin(position);
        Ok(())
    }

    pub fn visible(app: &AppHandle) -> Result<bool, String> {
        Ok(app
            .get_webview_panel("panel")
            .map_err(|e| format!("{e:?}"))?
            .is_visible())
    }

    pub fn focused(app: &AppHandle) -> Result<bool, String> {
        Ok(app
            .get_webview_panel("panel")
            .map_err(|e| format!("{e:?}"))?
            .as_panel()
            .isKeyWindow())
    }

    pub fn focus(app: &AppHandle) -> Result<(), String> {
        // 保留前台应用及其菜单栏；NSPanel 临时借用键盘焦点。插件也为 WKWebView
        // 安装了 NSTextInputClient.windowLevel，中文输入法候选窗可显示在面板上方。
        app.get_webview_panel("panel")
            .map_err(|e| format!("{e:?}"))?
            .show_and_make_key();
        // 已经打开过的搜索框仍是 DOM activeElement；再次 focus() 不一定会把
        // AppKit 的 first responder 从容器交回 WKWebView。显式恢复它，固定面板
        // 从其他应用取回焦点后才可以立即输入，同时不激活本应用。
        window(app)?
            .with_webview(|webview| {
                // SAFETY: with_webview 在主线程提供有效的 WKWebView；macOS 上
                // WKWebView 继承 NSView，借用只存在于此回调期间。
                let view = unsafe { &*(webview.inner() as *const objc2_app_kit::NSView) };
                if let Some(window) = view.window() {
                    window.makeFirstResponder(Some(view));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn hide(app: &AppHandle) -> Result<(), String> {
        app.get_webview_panel("panel")
            .map_err(|e| format!("{e:?}"))?
            .hide();
        Ok(())
    }
}

// Windows 本轮仅实现普通置顶窗口；未进行 Windows 编译或运行验证。
#[cfg(not(target_os = "macos"))]
mod native {
    use super::*;

    pub fn create(app: &tauri::App) -> tauri::Result<()> {
        tauri::WebviewWindowBuilder::new(app, "panel", tauri::WebviewUrl::App("panel.html".into()))
            .title("表情面板")
            .inner_size(WIDTH, HEIGHT)
            .decorations(false)
            .resizable(false)
            .visible(false)
            .focused(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .transparent(true)
            .shadow(true)
            .minimizable(false)
            .maximizable(false)
            .build()?;
        Ok(())
    }
    pub fn center_at_pointer(app: &AppHandle) -> Result<(), String> {
        let window = window(app)?;
        let cursor = window.cursor_position().map_err(|e| e.to_string())?;
        let monitors = window.available_monitors().map_err(|e| e.to_string())?;
        let monitor = monitors
            .iter()
            .find(|monitor| {
                let position = monitor.position();
                let size = monitor.size();
                cursor.x >= f64::from(position.x)
                    && cursor.x < f64::from(position.x) + f64::from(size.width)
                    && cursor.y >= f64::from(position.y)
                    && cursor.y < f64::from(position.y) + f64::from(size.height)
            })
            .ok_or("无法确定鼠标所在的屏幕")?;
        let area = monitor.work_area();
        window
            .set_position(centered_position(
                area.position,
                area.size,
                monitor.scale_factor(),
            ))
            .map_err(|e| e.to_string())
    }

    pub fn visible(app: &AppHandle) -> Result<bool, String> {
        window(app)?.is_visible().map_err(|e| e.to_string())
    }
    pub fn focused(app: &AppHandle) -> Result<bool, String> {
        window(app)?.is_focused().map_err(|e| e.to_string())
    }
    pub fn focus(app: &AppHandle) -> Result<(), String> {
        let window = window(app)?;
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())
    }
    pub fn hide(app: &AppHandle) -> Result<(), String> {
        window(app)?.hide().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_in_work_area_with_target_scale_and_negative_monitor_origin() {
        assert_eq!(
            centered_position(
                PhysicalPosition::new(-2560, 48),
                tauri::PhysicalSize::new(2560, 1352),
                2.0
            ),
            PhysicalPosition::new(-1640, 200)
        );
        assert_eq!(
            centered_position(
                PhysicalPosition::new(0, 24),
                tauri::PhysicalSize::new(1920, 1016),
                1.0
            ),
            PhysicalPosition::new(780, 270)
        );
    }
}
