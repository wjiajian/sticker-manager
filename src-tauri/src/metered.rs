//! 系统网络费用检测。失败按非计费处理；调试覆盖值不进入发布构建。
use tauri::AppHandle;

pub fn forced() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var("STICKER_FORCE_METERED").is_ok_and(|value| value == "1")
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

pub fn start(app: AppHandle) {
    if forced() {
        crate::sync::network_changed(&app, true);
        return;
    }
    native::start(app);
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use block2::{Block, RcBlock};
    use std::ffi::c_void;
    use tauri::Manager;

    // Network.framework 的 C API，无需创建 Swift 运行时或额外可执行文件。
    #[link(name = "Network", kind = "framework")]
    unsafe extern "C" {
        fn nw_path_monitor_create() -> *mut c_void;
        fn nw_path_monitor_set_queue(monitor: *mut c_void, queue: *mut c_void);
        fn nw_path_monitor_set_update_handler(
            monitor: *mut c_void,
            handler: &Block<dyn Fn(*mut c_void)>,
        );
        fn nw_path_monitor_start(monitor: *mut c_void);
        fn nw_path_monitor_cancel(monitor: *mut c_void);
        fn nw_path_is_expensive(path: *mut c_void) -> bool;
        fn nw_path_is_constrained(path: *mut c_void) -> bool;
        fn nw_release(object: *mut c_void);
    }
    unsafe extern "C" {
        fn dispatch_queue_create(
            label: *const std::ffi::c_char,
            attr: *const c_void,
        ) -> *mut c_void;
    }
    struct Monitor(usize);
    impl Drop for Monitor {
        fn drop(&mut self) {
            // 保留创建时的 +1 引用直到应用释放状态。
            unsafe {
                nw_path_monitor_cancel(self.0 as *mut c_void);
                nw_release(self.0 as *mut c_void);
            }
        }
    }
    pub fn start(app: AppHandle) {
        // monitor 复制 block，path 仅在回调期间读取。用串行队列，前后两次变化按顺序处理，
        // 不会让旧状态覆盖新状态；队列随应用存活，不释放。
        unsafe {
            let monitor = nw_path_monitor_create();
            if monitor.is_null() {
                return;
            }
            let handle = app.clone();
            let callback = RcBlock::new(move |path: *mut c_void| {
                let metered =
                    !path.is_null() && (nw_path_is_expensive(path) || nw_path_is_constrained(path));
                crate::sync::network_changed(&handle, metered);
            });
            nw_path_monitor_set_update_handler(monitor, &callback);
            let queue =
                dispatch_queue_create(c"sticker-manager.metered".as_ptr(), std::ptr::null());
            nw_path_monitor_set_queue(monitor, queue);
            nw_path_monitor_start(monitor);
            app.manage(Monitor(monitor as usize));
        }
    }
}

#[cfg(target_os = "windows")]
mod native {
    use super::*;
    use windows::Networking::Connectivity::{NetworkCostType, NetworkInformation};
    fn metered() -> windows::core::Result<bool> {
        let cost = NetworkInformation::GetInternetConnectionProfile()?.GetConnectionCost()?;
        Ok(cost.NetworkCostType()? != NetworkCostType::Unrestricted
            || cost.Roaming()?
            || cost.OverDataLimit()?)
    }
    pub fn start(app: AppHandle) {
        std::thread::spawn(move || {
            loop {
                crate::sync::network_changed(&app, metered().unwrap_or(false));
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod native {
    pub fn start(_: super::AppHandle) {}
}
