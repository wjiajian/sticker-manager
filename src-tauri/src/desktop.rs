//! 只在主线程调用的桌面坐标适配。
use crate::geometry::{Display, Point, Rect};
use tauri::AppHandle;
#[cfg(target_os = "macos")]
pub fn displays(_app: &AppHandle) -> Result<Vec<Display>, String> {
    use objc2::{MainThreadMarker, msg_send};
    use objc2_app_kit::NSScreen;
    use objc2_foundation::ns_string;
    let mtm = MainThreadMarker::new().ok_or("窗口操作必须在主线程进行")?;
    let screens = NSScreen::screens(mtm);
    let top = screens
        .firstObject()
        .ok_or("没有可用的显示器")?
        .frame()
        .size
        .height;
    Ok(screens
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let convert = |r: objc2_foundation::NSRect| Rect {
                x: r.origin.x,
                y: top - r.origin.y - r.size.height,
                width: r.size.width,
                height: r.size.height,
            };
            let id = s
                .deviceDescription()
                .objectForKey(ns_string!("NSScreenNumber"))
                .map(|n| {
                    // SAFETY: NSScreenNumber 是 AppKit 提供的 NSNumber。
                    let id: u32 = unsafe { msg_send![&*n, unsignedIntValue] };
                    id.to_string()
                })
                .unwrap_or_else(|| s.localizedName().to_string());
            Display {
                id,
                frame: convert(s.frame()),
                area: convert(s.visibleFrame()),
                scale: 1.0,
                primary: i == 0,
            }
        })
        .collect())
}
#[cfg(target_os = "macos")]
pub fn pointer(app: &AppHandle) -> Result<(Point, bool), String> {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceButtonState(state_id: i32, button: u32) -> bool;
    }
    let screens = displays(app)?;
    let top = screens
        .iter()
        .find(|s| s.primary)
        .ok_or("没有主显示器")?
        .frame
        .height;
    let p = objc2_app_kit::NSEvent::mouseLocation();
    Ok((
        Point {
            x: p.x,
            y: top - p.y,
        },
        // SAFETY: 读取 combined session (0) 的左键 (0)。NSEvent 的按钮掩码
        // 可能停留在本应用最后收到的事件，跨窗口松手必须查询系统级状态。
        unsafe { CGEventSourceButtonState(0, 0) },
    ))
}
#[cfg(target_os = "macos")]
pub fn set_frame(app: &AppHandle, label: &str, rect: Rect) -> Result<(), String> {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use tauri_nspanel::ManagerExt;
    let top = displays(app)?
        .into_iter()
        .find(|s| s.primary)
        .ok_or("没有主显示器")?
        .frame
        .height;
    app.get_webview_panel(label)
        .map_err(|e| format!("{e:?}"))?
        .as_panel()
        .setFrame_display(
            NSRect::new(
                NSPoint::new(rect.x, top - rect.y - rect.height),
                NSSize::new(rect.width, rect.height),
            ),
            true,
        );
    Ok(())
}
#[cfg(not(target_os = "macos"))]
pub fn displays(app: &AppHandle) -> Result<Vec<Display>, String> {
    let primary = app.primary_monitor().map_err(|e| e.to_string())?;
    Ok(app
        .available_monitors()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|m| {
            let area = m.work_area();
            Display {
                id: m.name().cloned().unwrap_or_default(),
                frame: Rect {
                    x: m.position().x as f64,
                    y: m.position().y as f64,
                    width: m.size().width as f64,
                    height: m.size().height as f64,
                },
                area: Rect {
                    x: area.position.x as f64,
                    y: area.position.y as f64,
                    width: area.size.width as f64,
                    height: area.size.height as f64,
                },
                scale: m.scale_factor(),
                primary: primary
                    .as_ref()
                    .is_some_and(|p| p.position() == m.position()),
            }
        })
        .collect())
}
#[cfg(not(target_os = "macos"))]
pub fn pointer(app: &AppHandle) -> Result<(Point, bool), String> {
    let p = app.cursor_position().map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    // SAFETY: GetAsyncKeyState 只读取鼠标键状态。
    let down = unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(1) } < 0;
    #[cfg(not(target_os = "windows"))]
    let down = false;
    Ok((Point { x: p.x, y: p.y }, down))
}
#[cfg(not(target_os = "macos"))]
pub fn set_frame(app: &AppHandle, label: &str, rect: Rect) -> Result<(), String> {
    use tauri::Manager;
    let w = app.get_webview_window(label).ok_or("窗口不可用")?;
    w.set_size(tauri::PhysicalSize::new(
        rect.width as u32,
        rect.height as u32,
    ))
    .map_err(|e| e.to_string())?;
    w.set_position(tauri::PhysicalPosition::new(rect.x as i32, rect.y as i32))
        .map_err(|e| e.to_string())
}
