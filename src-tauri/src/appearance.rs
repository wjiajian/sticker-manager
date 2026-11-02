//! 原生外观是唯一来源；WebView 的 prefers-color-scheme 随它变化。
use crate::settings::SettingsState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Theme, WindowEvent, webview::Color};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn theme(self) -> Option<Theme> {
        match self {
            Self::System => None,
            Self::Light => Some(Theme::Light),
            Self::Dark => Some(Theme::Dark),
        }
    }
}

pub fn preference(app: &AppHandle) -> Option<Theme> {
    app.state::<SettingsState>().get().appearance.theme()
}

// 与 global.css 的 surface 一致；在 WebView 首次绘制前也使用相同背景。
pub fn background(theme: Theme) -> Color {
    match theme {
        Theme::Dark => Color(24, 24, 24, 255),
        _ => Color(255, 255, 255, 255),
    }
}

pub fn apply(app: &AppHandle, appearance: Appearance) -> tauri::Result<()> {
    let theme = appearance.theme();
    // NSApplication 外观覆盖菜单、NSAlert、文件选择框以及隐藏的 NSPanel。
    app.set_theme(theme);
    for (label, window) in app.webview_windows() {
        // Windows 经 Tao ThemeChanged → Wry SetPreferredColorScheme 更新 WebView2。
        window.set_theme(theme)?;
        if label == "main" {
            window.set_background_color(Some(background(window.theme()?)))?;
        }
    }
    Ok(())
}

pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if window.label() == "main"
        && let WindowEvent::ThemeChanged(theme) = event
        && let Some(webview) = window.app_handle().get_webview_window("main")
        && let Err(error) = webview.set_background_color(Some(background(*theme)))
    {
        eprintln!("更新窗口背景失败：{error}");
    }
}
