//! 悬浮球的原生鼠标检测、吸附和收起。不能成为 key 的 WebView 不依赖 hover 事件。
use crate::{
    desktop,
    geometry::{self, BallFrame, Display, Point},
    panel,
    settings::{Edge, Settings, SettingsState},
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BallSnapshot {
    center: Point,
    edge: Edge,
    collapsed: bool,
    hovered: bool,
    pressed: bool,
    dragging: bool,
    panel_visible: bool,
    drop_label: Option<String>,
    message: Option<String>,
    tip_above: bool,
}
#[derive(Clone, Copy)]
struct Press {
    pointer: Point,
    center: Point,
}
struct Runtime {
    displays: Vec<Display>,
    display: Display,
    center: Point,
    edge: Edge,
    collapsed: bool,
    hovered: bool,
    dragging: bool,
    press: Option<Press>,
    idle_since: Instant,
    drop_label: Option<String>,
    message: Option<(String, Instant)>,
    menu_open: bool,
    last_frame: Option<BallFrame>,
    last_snapshot: Option<BallSnapshot>,
    accepts_mouse: Option<bool>,
}
struct BallState {
    runtime: Mutex<Runtime>,
    tick_queued: AtomicBool,
    // 鼠标靠近球或按住时加快轮询：拖动更跟手，放行点击的开关也切得更及时。
    fast: AtomicBool,
}

pub fn create(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let settings = app.state::<SettingsState>().get();
    let displays = desktop::displays(app.handle())?;
    let (display, center) =
        geometry::restore(&displays, &settings.ball_position).ok_or("没有可用的显示器")?;
    let runtime = Runtime {
        display: display.clone(),
        displays,
        center,
        edge: settings.ball_position.edge,
        collapsed: false,
        hovered: false,
        dragging: false,
        press: None,
        idle_since: Instant::now(),
        drop_label: None,
        message: None,
        menu_open: false,
        last_frame: None,
        last_snapshot: None,
        accepts_mouse: None,
    };
    app.manage(BallState {
        runtime: Mutex::new(runtime),
        tick_queued: AtomicBool::new(false),
        fast: AtomicBool::new(false),
    });
    native::create(app)?;
    settings_changed(app.handle(), &settings)?;
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        loop {
            let fast = handle.state::<BallState>().fast.load(Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(if fast { 16 } else { 50 }));
            // 避免菜单的原生事件循环或主线程繁忙时堆积轮询。也检测屏幕配置改变。
            if handle
                .state::<BallState>()
                .tick_queued
                .swap(true, Ordering::Relaxed)
            {
                continue;
            }
            let app = handle.clone();
            if handle
                .run_on_main_thread(move || {
                    if let Err(e) = tick(&app) {
                        eprintln!("更新悬浮球失败：{e}");
                    }
                    app.state::<BallState>()
                        .tick_queued
                        .store(false, Ordering::Relaxed);
                })
                .is_err()
            {
                break;
            }
        }
    });
    Ok(())
}

fn render(app: &AppHandle, r: &mut Runtime) -> Result<BallSnapshot, String> {
    let panel_visible = panel::is_visible(app)?;
    let frame = geometry::ball_frame(&r.display, r.center, r.edge);
    if r.last_frame.as_ref() != Some(&frame) {
        desktop::set_frame(app, "ball", frame.rect)?;
        r.last_frame = Some(frame.clone());
    }
    // 鼠标不在球上时整个窗口放行鼠标，透明区域的点击落到下面的窗口。
    let accepts = r.hovered || r.press.is_some() || r.drop_label.is_some();
    if r.accepts_mouse != Some(accepts) {
        native::set_accepts_mouse(app, accepts)?;
        r.accepts_mouse = Some(accepts);
    }
    let snapshot = BallSnapshot {
        center: if r.collapsed {
            geometry::collapsed(&r.display, &frame, r.center, r.edge).0
        } else {
            frame.center
        },
        edge: r.edge,
        collapsed: r.collapsed,
        hovered: r.hovered,
        pressed: r.press.is_some(),
        dragging: r.dragging,
        panel_visible,
        drop_label: r.drop_label.clone(),
        message: crate::imports::ball_progress(app)
            .or_else(|| r.message.as_ref().map(|m| m.0.clone())),
        tip_above: frame.tip_above,
    };
    if r.last_snapshot.as_ref() != Some(&snapshot) {
        app.emit_to("ball", "ball-state", &snapshot)
            .map_err(|e| e.to_string())?;
        r.last_snapshot = Some(snapshot.clone());
    }
    Ok(snapshot)
}

fn tick(app: &AppHandle) -> Result<(), String> {
    let settings = app.state::<SettingsState>().get();
    let state = app.state::<BallState>();
    let mut r = state.runtime.lock().unwrap();
    let displays = desktop::displays(app)?;
    if displays != r.displays {
        if let Some((display, center)) = geometry::restore(&displays, &settings.ball_position) {
            r.display = display.clone();
            r.center = center;
        }
        r.displays = displays;
        r.press = None;
        r.dragging = false;
        r.collapsed = false;
        r.idle_since = Instant::now();
    }
    if !settings.ball_visible {
        state.fast.store(false, Ordering::Relaxed);
        return Ok(());
    }
    let (pointer, down) = desktop::pointer(app)?;
    let mut save = None;
    let mut clicked = false;
    if let Some(press) = r.press {
        let dx = pointer.x - press.pointer.x;
        let dy = pointer.y - press.pointer.y;
        if !r.dragging && dx.hypot(dy) > 4.0 * r.display.scale {
            r.dragging = true;
            panel::hide_panel(app.clone(), false)?;
        }
        if r.dragging {
            r.center = Point {
                x: press.center.x + dx,
                y: press.center.y + dy,
            };
            if let Some(d) = geometry::display_at(&r.displays, r.center).cloned() {
                r.display = d;
            }
        }
        if !down {
            if r.dragging {
                let (position, center) = geometry::snap(&r.display, r.center);
                r.center = center;
                r.edge = position.edge;
                save = Some(position);
            } else {
                clicked = true;
            }
            r.dragging = false;
            r.press = None;
            r.idle_since = Instant::now();
        }
    }
    let distance = (pointer.x - r.center.x).hypot(pointer.y - r.center.y);
    let hit = if r.collapsed {
        let frame = geometry::ball_frame(&r.display, r.center, r.edge);
        geometry::collapsed(&r.display, &frame, r.center, r.edge)
            .1
            .contains(pointer)
    } else {
        distance <= (if r.drop_label.is_some() { 50.0 } else { 32.0 }) * r.display.scale
    };
    if hit != r.hovered {
        r.hovered = hit;
        r.idle_since = Instant::now();
    }
    state.fast.store(
        r.press.is_some() || r.drop_label.is_some() || distance <= 160.0 * r.display.scale,
        Ordering::Relaxed,
    );
    let import_progress = crate::imports::ball_progress(app).is_some();
    if import_progress {
        r.collapsed = false;
    }
    let panel_open = panel::is_visible(app)?;
    if r.hovered
        || r.press.is_some()
        || panel_open
        || r.drop_label.is_some()
        || r.menu_open
        || r.message.is_some()
        || import_progress
    {
        r.collapsed = false;
        r.idle_since = Instant::now();
    } else if let Some(seconds) = settings.ball_auto_hide_seconds {
        r.collapsed = r.idle_since.elapsed() >= Duration::from_secs(seconds as u64);
    } else {
        r.collapsed = false;
    }
    if r.message
        .as_ref()
        .is_some_and(|(_, until)| Instant::now() >= *until)
    {
        r.message = None;
    }
    render(app, &mut r)?;
    drop(r);
    if let Some(position) = save {
        crate::settings::update(app, |s| s.ball_position = position)?;
    }
    if clicked {
        toggle_panel(app.clone())?;
    }
    Ok(())
}

pub fn settings_changed(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let Some(state) = app.try_state::<BallState>() else {
        return Ok(());
    };
    let mut r = state.runtime.lock().unwrap();
    r.collapsed = false;
    r.idle_since = Instant::now();
    if !settings.ball_visible {
        r.press = None;
        r.dragging = false;
        r.drop_label = None;
        r.hovered = false;
    }
    render(app, &mut r)?;
    native::set_visible(app, settings.ball_visible)
}
#[tauri::command]
pub fn ball_state(app: AppHandle) -> Result<BallSnapshot, String> {
    render(&app, &mut app.state::<BallState>().runtime.lock().unwrap())
}
#[tauri::command]
pub fn ball_pointer_down(app: AppHandle) -> Result<(), String> {
    let (pointer, _) = desktop::pointer(&app)?;
    let state = app.state::<BallState>();
    let mut r = state.runtime.lock().unwrap();
    if r.menu_open {
        return Ok(());
    }
    r.collapsed = false;
    r.press = Some(Press {
        pointer,
        center: r.center,
    });
    r.idle_since = Instant::now();
    render(&app, &mut r)?;
    Ok(())
}
fn toggle_panel(app: AppHandle) -> Result<(), String> {
    if panel::is_visible(&app)? {
        panel::hide_panel(app, false)?;
        return Ok(());
    }
    let (display, center, edge) = {
        let state = app.state::<BallState>();
        let r = state.runtime.lock().unwrap();
        (r.display.clone(), r.center, r.edge)
    };
    panel::show_beside_ball(&app, &display, center, edge)
}
#[tauri::command]
pub fn set_ball_drop(app: AppHandle, paths: Option<Vec<PathBuf>>) -> Result<(), String> {
    let state = app.state::<BallState>();
    let mut r = state.runtime.lock().unwrap();
    r.drop_label = paths.filter(|p| !p.is_empty()).map(|paths| {
        let images = paths.iter().all(|p| {
            p.is_file()
                && p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    matches!(
                        e.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "webp" | "gif" | "apng"
                    )
                })
        });
        if images {
            format!("松开导入 {} 张", paths.len())
        } else {
            "松开导入".into()
        }
    });
    r.collapsed = false;
    r.idle_since = Instant::now();
    render(&app, &mut r)?;
    Ok(())
}
#[tauri::command]
pub fn ball_message(app: AppHandle, message: String) -> Result<(), String> {
    let state = app.state::<BallState>();
    let mut r = state.runtime.lock().unwrap();
    r.message = Some((message, Instant::now() + Duration::from_secs(2)));
    r.collapsed = false;
    render(&app, &mut r)?;
    Ok(())
}
pub fn menu_open(app: &AppHandle, open: bool) {
    let state = app.state::<BallState>();
    let mut r = state.runtime.lock().unwrap();
    r.menu_open = open;
    r.idle_since = Instant::now();
    // macOS 的 Control+单击先按下左键再弹菜单；菜单关闭时左键已松开，不能再算作单击。
    if open && !r.dragging {
        r.press = None;
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use tauri_nspanel::{
        CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask, tauri_panel,
    };
    tauri_panel! { panel!(StickerBall { config: {can_become_key_window:false,can_become_main_window:false,is_floating_panel:true} }) }
    pub fn create(app: &tauri::App) -> tauri::Result<()> {
        let theme = crate::appearance::preference(app.handle());
        PanelBuilder::<_, StickerBall>::new(app.handle(), "ball")
            .url(tauri::WebviewUrl::App("ball.html".into()))
            .title("悬浮球")
            .size(tauri::LogicalSize::new(300.0, 168.0).into())
            .resizable(false)
            .level(PanelLevel::Floating)
            .has_shadow(false)
            .transparent(true)
            .hides_on_deactivate(false)
            .becomes_key_only_if_needed(true)
            .add_style_mask(StyleMask::empty().nonactivating_panel())
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary()
                    .ignores_cycle()
                    // 单击墙纸显示桌面、调度中心时留在原处，用户仍能把桌面上的图片拖到球上。
                    .stationary(),
            )
            .with_window(move |w| {
                w.theme(theme)
                    .decorations(false)
                    // PanelBuilder 的 transparent 只清除 NSPanel 背景；WKWebView 也必须透明。
                    .transparent(true)
                    .visible(false)
                    .focused(false)
                    .focusable(false)
                    .accept_first_mouse(true)
                    .skip_taskbar(true)
                    .minimizable(false)
                    .maximizable(false)
            })
            .build()?;
        Ok(())
    }
    pub fn set_visible(app: &AppHandle, visible: bool) -> Result<(), String> {
        let panel = app
            .get_webview_panel("ball")
            .map_err(|e| format!("{e:?}"))?;
        if visible {
            panel.show();
        } else {
            panel.hide();
        }
        Ok(())
    }
    pub fn set_accepts_mouse(app: &AppHandle, accepts: bool) -> Result<(), String> {
        app.get_webview_panel("ball")
            .map_err(|e| format!("{e:?}"))?
            .set_ignores_mouse_events(!accepts);
        Ok(())
    }
}
#[cfg(not(target_os = "macos"))]
mod native {
    use super::*;
    pub fn create(app: &tauri::App) -> tauri::Result<()> {
        let builder = tauri::WebviewWindowBuilder::new(
            app,
            "ball",
            tauri::WebviewUrl::App("ball.html".into()),
        )
        .theme(crate::appearance::preference(app.handle()))
        .title("悬浮球")
        .inner_size(300.0, 168.0)
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
        builder.build()?;
        Ok(())
    }
    pub fn set_visible(app: &AppHandle, visible: bool) -> Result<(), String> {
        let w = app.get_webview_window("ball").ok_or("悬浮球不可用")?;
        (if visible { w.show() } else { w.hide() }).map_err(|e| e.to_string())
    }
    pub fn set_accepts_mouse(app: &AppHandle, accepts: bool) -> Result<(), String> {
        let w = app.get_webview_window("ball").ok_or("悬浮球不可用")?;
        w.set_ignore_cursor_events(!accepts)
            .map_err(|e| e.to_string())
    }
}
