//! 只显示内容的预览窗口。页面提交并解码缩略图后才移动、显示窗口。
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::geometry::{PanelOrigin, Point, Rect, preview_position};

const INSET: f64 = 64.0;
const WIDTH: f64 = 168.0 + INSET * 2.0;
const HEIGHT: f64 = 232.0 + INSET * 2.0 + 32.0;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSticker {
    name: String,
    usage_count: u64,
    category: Option<String>,
    thumbnail_path: String,
    object_path: String,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct TileAnchor {
    top: f64,
    height: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    request_id: u64,
    sticker: PreviewSticker,
    #[serde(skip_serializing)]
    anchor: TileAnchor,
    fade: bool,
}

#[derive(Default)]
struct PreviewState {
    serial: u64,
    pending: Option<PreviewRequest>,
    visible: bool,
}

impl PreviewState {
    fn dismiss(&mut self) {
        self.serial += 1;
        self.pending = None;
        self.visible = false;
    }

    fn current(&self, request_id: u64) -> Option<&PreviewRequest> {
        self.pending.as_ref().filter(|r| r.request_id == request_id)
    }
}

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    app.manage(Mutex::new(PreviewState::default()));
    native::create(app)
}

pub fn dismiss(app: &AppHandle) -> Result<(), String> {
    let Some(state) = app.try_state::<Mutex<PreviewState>>() else {
        return Ok(());
    };
    let mut state = state.lock().unwrap();
    state.dismiss();
    native::hide(app)?;
    if let Some(window) = app.get_webview_window("preview") {
        window
            .emit("preview-hidden", state.serial)
            .map_err(|e| e.to_string())?;
    }
    if let Some(window) = app.get_webview_window("panel") {
        window
            .emit("preview-visibility", false)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn reset(app: &AppHandle) -> Result<(), String> {
    dismiss(app)?;
    if let Some(window) = app.get_webview_window("panel") {
        window
            .emit("preview-reset", ())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn show_preview(
    app: AppHandle,
    sticker: PreviewSticker,
    anchor: TileAnchor,
) -> Result<(), String> {
    if !crate::panel::can_preview(&app)? {
        return Ok(());
    }
    if !anchor.top.is_finite() || !anchor.height.is_finite() || anchor.height <= 0.0 {
        return Err("预览格子位置无效".into());
    }
    let state = app.state::<Mutex<PreviewState>>();
    let mut state = state.lock().unwrap();
    state.serial += 1;
    let request = PreviewRequest {
        request_id: state.serial,
        sticker,
        anchor,
        fade: !state.visible,
    };
    state.pending = Some(request.clone());
    app.get_webview_window("preview")
        .ok_or("预览窗口不可用")?
        .emit("preview-content", request)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn hide_preview(app: AppHandle) -> Result<(), String> {
    dismiss(&app)
}

/// 先安装监听，再取待显示内容，覆盖预览 WebView 尚未启动完成的情况。
#[tauri::command]
pub fn preview_session(app: AppHandle) -> Option<PreviewRequest> {
    app.state::<Mutex<PreviewState>>()
        .lock()
        .unwrap()
        .pending
        .clone()
}

#[tauri::command]
pub fn preview_ready(app: AppHandle, request_id: u64, card_height: f64) -> Result<bool, String> {
    if !card_height.is_finite() || !(216.0..=232.0).contains(&card_height) {
        return Err("预览卡片尺寸无效".into());
    }
    let state = app.state::<Mutex<PreviewState>>();
    let mut state = state.lock().unwrap();
    let Some(request) = state.current(request_id) else {
        return Ok(false);
    };
    if !crate::panel::can_preview(&app)? {
        return Ok(false);
    }
    let position = native::position(
        &app,
        request.anchor,
        card_height,
        crate::panel::origin(&app),
    )?;
    let Some(position) = position else {
        drop(state);
        dismiss(&app)?;
        return Ok(false);
    };
    native::show_at(&app, position)?;
    state.visible = true;
    app.get_webview_window("panel")
        .ok_or("表情面板不可用")?
        .emit("preview-visibility", true)
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use objc2_app_kit::NSScreen;
    use tauri_nspanel::{
        CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask, tauri_panel,
    };

    tauri_panel! {
        panel!(PreviewPanel {
            config: {
                can_become_key_window: false,
                can_become_main_window: false,
                is_floating_panel: true
            }
        })
    }

    pub fn create(app: &tauri::App) -> tauri::Result<()> {
        let theme = crate::appearance::preference(app.handle());
        PanelBuilder::<_, PreviewPanel>::new(app.handle(), "preview")
            .url(tauri::WebviewUrl::App("preview.html".into()))
            .title("表情预览")
            .size(tauri::LogicalSize::new(WIDTH, HEIGHT).into())
            .resizable(false)
            .level(PanelLevel::Custom(PanelLevel::Floating.value() as i32 + 1))
            .has_shadow(false)
            .transparent(true)
            .hides_on_deactivate(false)
            .ignores_mouse_events(true)
            .add_style_mask(StyleMask::empty().nonactivating_panel())
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary()
                    .ignores_cycle(),
            )
            .with_window(move |w| {
                w.theme(theme)
                    .decorations(false)
                    .transparent(true)
                    .visible(false)
                    .focused(false)
                    .focusable(false)
                    .skip_taskbar(true)
                    .minimizable(false)
                    .maximizable(false)
            })
            .build()?;
        Ok(())
    }

    pub fn position(
        app: &AppHandle,
        anchor: TileAnchor,
        height: f64,
        origin: PanelOrigin,
    ) -> Result<Option<Point>, String> {
        let mtm = MainThreadMarker::new().ok_or("窗口操作必须在主线程进行")?;
        let panel = app
            .get_webview_panel("panel")
            .map_err(|e| format!("{e:?}"))?;
        let frame = panel.as_panel().frame();
        let convert = |r: NSRect| Rect {
            x: r.origin.x,
            y: r.origin.y,
            width: r.size.width,
            height: r.size.height,
        };
        let frame = convert(frame);
        let center = Point {
            x: frame.x + frame.width / 2.0,
            y: frame.y + frame.height / 2.0,
        };
        let screens = NSScreen::screens(mtm);
        let screen = screens
            .iter()
            .find(|s| convert(s.frame()).contains(center))
            .ok_or("无法确定面板所在屏幕")?;
        // 全程使用 AppKit 左下原点、逻辑像素，只在这里翻转前端的 CSS y 坐标。
        Ok(preview_position(
            convert(screen.visibleFrame()),
            frame,
            origin,
            frame.y + frame.height - anchor.top - anchor.height / 2.0,
            height,
        )
        .map(|card| Point {
            x: card.x - INSET,
            y: card.y - (HEIGHT - INSET - height),
        }))
    }

    pub fn show_at(app: &AppHandle, point: Point) -> Result<(), String> {
        let panel = app
            .get_webview_panel("preview")
            .map_err(|e| format!("{e:?}"))?;
        panel
            .as_panel()
            .setFrameOrigin(NSPoint::new(point.x, point.y));
        panel.show();
        Ok(())
    }

    pub fn hide(app: &AppHandle) -> Result<(), String> {
        app.get_webview_panel("preview")
            .map_err(|e| format!("{e:?}"))?
            .hide();
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
mod native {
    use super::*;

    pub fn create(app: &tauri::App) -> tauri::Result<()> {
        let builder = tauri::WebviewWindowBuilder::new(
            app,
            "preview",
            tauri::WebviewUrl::App("preview.html".into()),
        )
        .theme(crate::appearance::preference(app.handle()))
        .title("表情预览")
        .inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .resizable(false)
        .visible(false)
        .focused(false)
        .focusable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(true)
        .shadow(false)
        .minimizable(false)
        .maximizable(false);
        #[cfg(target_os = "windows")]
        let builder = match crate::portable::root(app.handle()) {
            Some(root) => builder.data_directory(root.join("webview")),
            None => builder,
        };
        let window = builder.build()?;
        window.set_ignore_cursor_events(true)
    }

    pub fn position(
        app: &AppHandle,
        anchor: TileAnchor,
        height: f64,
        origin: PanelOrigin,
    ) -> Result<Option<Point>, String> {
        let panel = app.get_webview_window("panel").ok_or("表情面板不可用")?;
        let monitor = panel
            .current_monitor()
            .map_err(|e| e.to_string())?
            .ok_or("无法确定面板所在屏幕")?;
        let scale = monitor.scale_factor();
        let p = panel.outer_position().map_err(|e| e.to_string())?;
        let size = panel.outer_size().map_err(|e| e.to_string())?;
        let area = monitor.work_area();
        let frame = Rect {
            x: f64::from(p.x) / scale,
            y: f64::from(p.y) / scale,
            width: f64::from(size.width) / scale,
            height: f64::from(size.height) / scale,
        };
        let area = Rect {
            x: f64::from(area.position.x) / scale,
            y: f64::from(area.position.y) / scale,
            width: f64::from(area.size.width) / scale,
            height: f64::from(area.size.height) / scale,
        };
        Ok(preview_position(
            area,
            frame,
            origin,
            frame.y + anchor.top + anchor.height / 2.0,
            height,
        )
        .map(|card| Point {
            x: (card.x - INSET) * scale,
            y: (card.y - INSET) * scale,
        }))
    }

    pub fn show_at(app: &AppHandle, point: Point) -> Result<(), String> {
        let window = app.get_webview_window("preview").ok_or("预览窗口不可用")?;
        window
            .set_position(tauri::PhysicalPosition::new(
                point.x.round() as i32,
                point.y.round() as i32,
            ))
            .map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())
    }

    pub fn hide(app: &AppHandle) -> Result<(), String> {
        app.get_webview_window("preview")
            .ok_or("预览窗口不可用")?
            .hide()
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_and_dismissal_reject_old_ready_replies() {
        let request = PreviewRequest {
            request_id: 2,
            sticker: PreviewSticker {
                name: "猫猫".into(),
                usage_count: 0,
                category: None,
                thumbnail_path: "thumb".into(),
                object_path: "original".into(),
            },
            anchor: TileAnchor {
                top: 40.0,
                height: 78.0,
            },
            fade: false,
        };
        let mut state = PreviewState {
            serial: 2,
            pending: Some(request),
            visible: true,
        };
        assert!(state.current(1).is_none());
        assert!(state.current(2).is_some());
        state.dismiss();
        assert!(state.current(2).is_none());
        assert!(!state.visible);
        assert_eq!(state.serial, 3);
    }
}
