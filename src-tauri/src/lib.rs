use std::sync::Mutex;

use sticker_core::Library;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

mod commands;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // macOS：~/Library/Application Support/<identifier>/；Windows：%APPDATA%\<identifier>\
            let library = Library::open(&app.path().app_data_dir()?)?;
            app.manage(Mutex::new(library));
            create_main_window(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::library_overview])
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
