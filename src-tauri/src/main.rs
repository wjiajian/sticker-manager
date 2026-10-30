// 发布版在 Windows 上不额外弹出控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    sticker_manager_lib::run()
}
