use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use sticker_core::Library;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

mod appearance;
#[cfg(target_os = "macos")]
mod autostart;
mod ball;
mod clipboard;
mod commands;
mod credentials;
mod desktop;
mod export;
mod exports;
mod geometry;
mod identifier;
mod imports;
mod jobs;
mod location;
#[cfg(target_os = "macos")]
mod menu;
mod metered;
mod panel;
mod portable;
mod preview;
mod settings;
mod shortcut;
mod startup;
mod sync;
mod tray;
mod updater;
#[cfg(target_os = "windows")]
mod windows_drop;

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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let portable = match portable::Portable::detect() {
                Ok(portable) => portable,
                Err(error) => {
                    let path = std::env::current_exe()
                        .ok()
                        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
                        .unwrap_or_else(|| PathBuf::from("（程序所在目录）"));
                    app.manage(startup::StartupFailure(Mutex::new(Some((
                        path,
                        format!("无法初始化便携目录，请确认程序所在目录可写：{error}"),
                    )))));
                    return Ok(());
                }
            };
            app.manage(portable);
            let (mut library, root, location) = match open_library(app) {
                Ok(library) => library,
                Err((path, reason)) => {
                    app.set_theme(settings::Settings::load(&path).appearance.theme());
                    app.manage(startup::StartupFailure(Mutex::new(Some((path, reason)))));
                    return Ok(());
                }
            };
            app.manage(location);
            app.manage(location::MoveState::default());
            app.manage(jobs::LibraryJobs::default());
            app.manage(exports::ExportState::default());
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
            app.manage(updater::UpdateState::new(
                app.package_info().version.to_string(),
            ));
            app.manage(export::ExportDirectory::new(
                std::env::temp_dir()
                    .join(app.config().identifier.clone())
                    .join("exports"),
            ));
            // WebView2 可在 setup 结束前发出查询，先登记同步状态。
            #[cfg(target_os = "windows")]
            sync::start(app.handle().clone());
            panel::create(app)?;
            preview::create(app)?;
            shortcut::init(app.handle());
            create_main_window(app)?;
            tray::create(app)?;
            ball::create(app)?;
            settings::initialize_autostart(app.handle());
            #[cfg(not(target_os = "windows"))]
            sync::start(app.handle().clone());
            updater::start(app.handle().clone());
            start_expiry_cleanup(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            shortcut::on_window_event(window, event);
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
            commands::move_category,
            commands::set_sticker_category,
            commands::set_sticker_name,
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
            commands::set_stickers_favorite,
            commands::reveal_sticker,
            panel::panel_session,
            panel::set_panel_pinned,
            panel::set_panel_dragging,
            panel::hide_panel,
            panel::show_main_window,
            shortcut::panel_shortcut_state,
            shortcut::check_panel_shortcut,
            shortcut::set_panel_shortcut,
            shortcut::begin_panel_shortcut_recording,
            shortcut::end_panel_shortcut_recording,
            preview::show_preview,
            preview::hide_preview,
            preview::preview_session,
            preview::preview_ready,
            settings::get_settings,
            settings::set_appearance,
            settings::set_ball_visible,
            settings::set_ball_auto_hide,
            settings::set_ball_size,
            settings::set_sticker_tile_size,
            settings::get_autostart,
            settings::set_autostart,
            exports::export_stickers,
            exports::export_status,
            exports::cancel_export,
            exports::reveal_export,
            location::library_location,
            location::inspect_library_target,
            location::move_library,
            location::library_move_status,
            location::cancel_library_move,
            settings::open_library_directory,
            tray::take_preferences_request,
            tray::show_ball_menu,
            ball::ball_state,
            ball::ball_pointer_down,
            ball::set_ball_drop,
            ball::ball_message,
            ball::preview_ball_size,
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
            updater::update_status,
            updater::check_update,
            updater::install_update,
            updater::ignore_update,
            updater::set_auto_check_updates,
            updater::take_update_request,
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
                    commands::notify_library_changed(&app);
                }
                Ok(_) => {}
                Err(error) => eprintln!("清理最近删除失败：{error}"),
            }
        }
    });
}

fn open_library(
    app: &tauri::App,
) -> Result<(Library, PathBuf, location::LocationState), (PathBuf, String)> {
    use location::LocationMode;
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| (PathBuf::from("（无法确定仓库路径）"), e.to_string()))?;
    #[cfg(windows)]
    let default = data.with_file_name("sticker-manager");
    #[cfg(not(windows))]
    let default = data.clone();
    let (base, mode, guarded) = if cfg!(debug_assertions)
        && let Some(path) = std::env::var_os("STICKER_LIBRARY_DIR").filter(|s| !s.is_empty())
    {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err((path, "STICKER_LIBRARY_DIR 必须是绝对路径".into()));
        }
        #[cfg(windows)]
        let guarded = vec![data, default];
        #[cfg(not(windows))]
        let guarded = vec![data];
        (path, LocationMode::Override, guarded)
    } else if let Some(portable) = portable::root(app.handle()) {
        (portable.join("data"), LocationMode::Portable, vec![])
    } else {
        (default, LocationMode::Default, vec![])
    };
    let (root, custom) = location::resolve_root(&base, mode)?;
    if guarded
        .iter()
        .filter_map(|p| p.canonicalize().ok())
        .any(|p| root.canonicalize().is_ok_and(|r| r.starts_with(p)))
    {
        return Err((root, "调试时不能访问真实仓库".into()));
    }
    if !custom && mode == LocationMode::Default {
        #[cfg(windows)]
        let result = identifier::migrate_windows_library(&root);
        #[cfg(not(windows))]
        let result = identifier::migrate_previous_library(&root);
        result.map_err(|e| (root.clone(), format!("无法把旧仓库搬到新位置：{e}")))?;
    }
    let library = if custom { Library::open_existing(&root) } else { Library::open(&root) }.map_err(|error| {
        let denied = matches!(&error, sticker_core::Error::Io(io) if io.kind() == std::io::ErrorKind::PermissionDenied);
        let mut reason = startup::reason(error);
        if custom && denied {
            let hint = if cfg!(target_os = "macos") {
                "。请在系统设置 › 隐私与安全性 › 文件与文件夹中允许访问仓库。"
            } else {
                "。请检查文件夹的访问权限。"
            };
            reason.push_str(hint);
        }
        (root.clone(), reason)
    })?;
    for d in ["objects", "thumbs"] {
        app.asset_protocol_scope()
            .allow_directory(root.join(d), false)
            .map_err(|e| (root.clone(), e.to_string()))?
    }
    let path = sticker_core::relocate::display_path(library.root());
    Ok((
        library,
        path,
        location::LocationState {
            base,
            mode,
            guarded,
        },
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
            if payload.event() == tauri::webview::PageLoadEvent::Started {
                shortcut::cancel_recording(window.app_handle());
            }
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

    #[cfg(target_os = "windows")]
    let builder = builder
        .decorations(false)
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../icons/windows/icon.png"
        ))?)?;

    #[cfg(target_os = "windows")]
    let builder = match portable::root(app.handle()) {
        Some(root) => builder.data_directory(root.join("webview")),
        None => builder,
    };

    builder.build()?;
    Ok(())
}
