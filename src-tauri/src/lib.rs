use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use sticker_core::Library;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

mod appearance;
#[cfg(target_os = "macos")]
mod autostart;
mod ball;
mod clipboard;
mod commands;
mod credentials;
mod desktop;
mod export;
mod geometry;
mod imports;
#[cfg(target_os = "macos")]
mod menu;
mod metered;
mod panel;
mod preview;
mod settings;
mod startup;
mod sync;
mod tray;

pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init()).menu(menu::build);

    let autostart = tauri_plugin_autostart::Builder::new().args(["--autostart"]);
    #[cfg(target_os = "macos")]
    let autostart = autostart.macos_launcher(tauri_plugin_autostart::MacosLauncher::LaunchAgent);

    builder
        .plugin(autostart.build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let (mut library, root) = match open_library(app) {
                Ok(library) => library,
                Err((path, reason)) => {
                    app.set_theme(settings::Settings::load(&path).appearance.theme());
                    app.manage(startup::StartupFailure(Mutex::new(Some((path, reason)))));
                    return Ok(());
                }
            };
            app.manage(settings::SettingsState::new(root));
            app.set_theme(appearance::preference(app.handle()));
            if let Err(error) = library.purge_expired() {
                eprintln!("清理最近删除失败：{error}");
            }
            if let Err(error) = library.sweep_orphan_files() {
                eprintln!("清理残留文件失败：{error}");
            }
            let sync = app.state::<settings::SettingsState>().get().sync;
            let maintenance = std::time::Instant::now();
            if let Err(error) = library
                .maintain_sync(sync.as_ref().map(|s| s.library_id.as_str()))
                .and_then(|()| library.compact_database_at_startup())
            {
                eprintln!("启动时清理本地日志失败：{error}");
            }
            eprintln!("启动时本地日志维护：{:?}", maintenance.elapsed());
            app.manage(Mutex::new(library));
            app.manage(imports::ImportState::default());
            app.manage(export::ExportDirectory::new(
                std::env::temp_dir()
                    .join(app.config().identifier.clone())
                    .join("exports"),
            ));
            panel::create(app)?;
            preview::create(app)?;
            panel::register_shortcut(app.handle());
            create_main_window(app)?;
            tray::create(app)?;
            ball::create(app)?;
            settings::initialize_autostart(app.handle());
            sync::start(app.handle().clone());
            start_expiry_cleanup(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            panel::on_window_event(window, event);
            appearance::on_window_event(window, event);
        })
        .invoke_handler(tauri::generate_handler![
            commands::library_overview,
            commands::list_stickers,
            commands::list_stickers_page,
            commands::search_stickers,
            commands::sticker,
            commands::delete_sticker,
            commands::restore_sticker,
            commands::delete_stickers,
            commands::restore_stickers,
            commands::purge_sticker,
            commands::purge_deleted_stickers,
            commands::categories,
            commands::create_category,
            commands::rename_category,
            commands::set_category_color,
            commands::delete_category,
            commands::set_sticker_category,
            commands::set_sticker_caption,
            commands::import_stickers,
            commands::import_status,
            commands::cancel_import,
            commands::set_stickers_category,
            commands::create_category_and_set_stickers,
            commands::copy_sticker,
            commands::prepare_sticker_drag,
            commands::record_sticker_usage,
            commands::toggle_sticker_favorite,
            commands::reveal_sticker,
            panel::panel_session,
            panel::set_panel_pinned,
            panel::set_panel_dragging,
            panel::hide_panel,
            panel::show_main_window,
            panel::global_shortcut_error,
            preview::show_preview,
            preview::hide_preview,
            preview::preview_session,
            preview::preview_ready,
            settings::get_settings,
            settings::set_appearance,
            settings::set_ball_visible,
            settings::set_ball_auto_hide,
            settings::get_autostart,
            settings::set_autostart,
            settings::library_directory,
            settings::open_library_directory,
            tray::take_preferences_request,
            tray::show_ball_menu,
            ball::ball_state,
            ball::ball_pointer_down,
            ball::set_ball_drop,
            ball::ball_message,
            sync::sync_status,
            sync::sync_now,
            sync::set_sync_paused,
            sync::set_pause_on_metered,
            sync::sticker_sync_info,
            tray::show_sync_details,
            tray::take_sync_details_request,
            sync::inspect_sync_folder,
            sync::join_sync_folder,
            sync::stop_sync,
            sync::inspect_network_target,
            sync::join_network_target,
            sync::cancel_sync_connection,
            sync::update_sync_credentials,
            sync::set_device_name,
        ])
        .build(tauri::generate_context!())
        .expect("启动应用失败")
        .run(|_app, _event| {
            if matches!(_event, tauri::RunEvent::Ready) {
                #[cfg(target_os = "macos")]
                menu::install_filters();
                startup::show_pending(_app);
            }
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event
                && let Err(error) = panel::show_main_window(_app.clone())
            {
                eprintln!("无法打开主窗口：{error}");
            }
        });
}

fn start_expiry_cleanup(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(60 * 60));
            let state = app.state::<Mutex<Library>>();
            let result = match state.lock() {
                Ok(mut library) => {
                    let result = library.purge_expired();
                    if let Err(error) = library.sweep_orphan_files() {
                        eprintln!("清理残留文件失败：{error}");
                    }
                    result
                }
                Err(error) => {
                    eprintln!("清理最近删除失败：{error}");
                    continue;
                }
            };
            match result {
                Ok(count) if count > 0 => {
                    sync::local_changed(&app);
                    if let Err(error) = app.emit("library-changed", ()) {
                        eprintln!("通知仓库更新失败：{error}");
                    }
                }
                Ok(_) => {}
                Err(error) => eprintln!("清理最近删除失败：{error}"),
            }
        }
    });
}

/// 开发构建里设置了这个环境变量，就改用它指向的目录做仓库，调试和验证时不碰真实仓库。
#[cfg(debug_assertions)]
const LIBRARY_DIR_OVERRIDE: &str = "STICKER_LIBRARY_DIR";

fn open_library(app: &tauri::App) -> Result<(Library, PathBuf), (PathBuf, String)> {
    #[cfg(debug_assertions)]
    if let Some(root) = std::env::var_os(LIBRARY_DIR_OVERRIDE).filter(|root| !root.is_empty()) {
        let root = PathBuf::from(root);
        // 相对路径会落在 src-tauri/ 下，写入的文件又会触发 tauri dev 重新编译
        if !root.is_absolute() {
            let reason = format!("{LIBRARY_DIR_OVERRIDE} 必须是绝对路径：{}", root.display());
            return Err((root, reason));
        }
        let library =
            Library::open(&root).map_err(|error| (root.clone(), startup::reason(error)))?;
        // tauri.conf.json 里的 asset 协议范围只覆盖默认仓库，换了位置要把新目录也放进去
        let scope = app.asset_protocol_scope();
        scope
            .allow_directory(root.join("objects"), false)
            .map_err(|error| (root.clone(), error.to_string()))?;
        scope
            .allow_directory(root.join("thumbs"), false)
            .map_err(|error| (root.clone(), error.to_string()))?;
        eprintln!("仓库：{}（来自 {LIBRARY_DIR_OVERRIDE}）", root.display());
        return Ok((library, root));
    }

    // macOS：~/Library/Application Support/<identifier>/；Windows：%APPDATA%\<identifier>\
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| (PathBuf::from("（无法确定仓库路径）"), error.to_string()))?;
    Ok((
        Library::open(&root).map_err(|error| (root.clone(), startup::reason(error)))?,
        root,
    ))
}

fn create_main_window(app: &tauri::App) -> tauri::Result<()> {
    let theme = appearance::preference(app.handle());
    let resolved = theme.unwrap_or(app.get_webview_window("panel").unwrap().theme()?);
    let show = !std::env::args().any(|arg| arg == "--autostart");
    // 只在第一次载入完成时显示；之后重新载入（比如开发时整页刷新）不把用户隐藏的窗口弹出来。
    let first_load = std::sync::Once::new();
    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title("表情管理器")
        .theme(theme)
        .background_color(appearance::background(resolved))
        .visible(false)
        .focused(show)
        .on_page_load(move |window, payload| {
            if show && payload.event() == tauri::webview::PageLoadEvent::Finished {
                first_load.call_once(|| {
                    let _ = window.show();
                });
            }
        })
        .inner_size(1280.0, 800.0)
        .min_inner_size(960.0, 600.0);

    // 标题栏和页面顶栏合成一条，红绿灯按钮对齐到顶栏的垂直中线
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(16.0, 33.0));

    builder.build()?;
    Ok(())
}
