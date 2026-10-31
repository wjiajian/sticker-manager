#[cfg(debug_assertions)]
use std::path::PathBuf;
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
            let library = open_library(app)?;
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
            commands::search_stickers,
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

/// 开发构建里设置了这个环境变量，就改用它指向的目录做仓库，调试和验证时不碰真实仓库。
#[cfg(debug_assertions)]
const LIBRARY_DIR_OVERRIDE: &str = "STICKER_LIBRARY_DIR";

fn open_library(app: &tauri::App) -> Result<Library, Box<dyn std::error::Error>> {
    #[cfg(debug_assertions)]
    if let Some(root) = std::env::var_os(LIBRARY_DIR_OVERRIDE).filter(|root| !root.is_empty()) {
        let root = PathBuf::from(root);
        // 相对路径会落在 src-tauri/ 下，写入的文件又会触发 tauri dev 重新编译
        if !root.is_absolute() {
            return Err(
                format!("{LIBRARY_DIR_OVERRIDE} 必须是绝对路径：{}", root.display()).into(),
            );
        }
        let library = Library::open(&root)?;
        // tauri.conf.json 里的 asset 协议范围只覆盖默认仓库，换了位置要把新目录也放进去
        let scope = app.asset_protocol_scope();
        scope.allow_directory(root.join("objects"), false)?;
        scope.allow_directory(root.join("thumbs"), false)?;
        eprintln!("仓库：{}（来自 {LIBRARY_DIR_OVERRIDE}）", root.display());
        return Ok(library);
    }

    // macOS：~/Library/Application Support/<identifier>/；Windows：%APPDATA%\<identifier>\
    Ok(Library::open(&app.path().app_data_dir()?)?)
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
        .traffic_light_position(tauri::LogicalPosition::new(16.0, 33.0));

    builder.build()?;
    Ok(())
}
