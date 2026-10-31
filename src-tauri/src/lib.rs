use std::sync::Mutex;

use sticker_core::Library;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

mod clipboard;
mod commands;
mod export;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // macOS：~/Library/Application Support/<identifier>/；Windows：%APPDATA%\<identifier>\
            let library = Library::open(&app.path().app_data_dir()?)?;
            app.manage(Mutex::new(library));
            app.manage(export::ExportDirectory::new(
                std::env::temp_dir()
                    .join(app.config().identifier.clone())
                    .join("exports"),
            ));
            create_main_window(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::library_overview,
            commands::list_stickers,
            commands::sticker,
            commands::tag_sidebar,
            commands::create_tag,
            commands::rename_tag,
            commands::set_tag_color,
            commands::move_tag,
            commands::delete_tag,
            commands::create_tag_group,
            commands::rename_tag_group,
            commands::delete_tag_group,
            commands::add_sticker_tag,
            commands::remove_sticker_tag,
            commands::create_tag_and_add,
            commands::set_sticker_caption,
            commands::import_stickers,
            commands::copy_sticker,
            commands::prepare_sticker_drag,
            commands::record_sticker_usage,
            commands::toggle_sticker_favorite,
            commands::reveal_sticker,
        ])
        .run(tauri::generate_context!())
        .expect("启动应用失败");
}

fn create_main_window(app: &tauri::App) -> tauri::Result<()> {
    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title("表情管理器")
        .inner_size(1280.0, 800.0)
        .min_inner_size(960.0, 600.0);

    // 标题栏和页面顶栏合成一条，红绿灯按钮对齐到顶栏的垂直中线
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(16.0, 25.0));

    builder.build()?;
    Ok(())
}
