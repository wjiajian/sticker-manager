//! macOS 的拖拽可以提供图片数据或文件承诺，而不只是 Wry 能读取的文件路径。
use std::path::{Path, PathBuf};

const MAX_FILES: usize = 256;
const MAX_BYTES: u64 = 100 * 1024 * 1024;

// 只导入本次接收目录中的普通文件。文件承诺回调返回的 URL 由外部进程提供。
fn validate_paths(root: &Path, paths: &[PathBuf]) -> Result<u64, String> {
    if paths.is_empty() || paths.len() > MAX_FILES {
        return Err("拖入文件为空或超过 256 项".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut total = 0u64;
    for path in paths {
        let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !metadata.file_type().is_file()
            || !path
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(&root)
        {
            return Err("拖入文件不在本次接收目录中".into());
        }
        total = total.checked_add(metadata.len()).ok_or("拖入文件过大")?;
        if total > MAX_BYTES {
            return Err("拖入文件总量超过 100 MB".into());
        }
    }
    Ok(total)
}

#[cfg(target_os = "macos")]
mod native {
    use super::validate_paths;
    use std::{
        ffi::{CStr, CString, c_char, c_void},
        os::unix::ffi::OsStrExt,
        path::PathBuf,
        sync::{Arc, Mutex, OnceLock, mpsc},
    };
    use tauri::{AppHandle, Emitter, Manager};

    unsafe extern "C" {
        fn sticker_macos_install(view: *mut c_void, app: *mut c_void) -> bool;
    }

    struct Batch {
        app: AppHandle,
        temporary: tempfile::TempDir,
        destination: CString,
        budget: Mutex<(usize, u64)>,
    }
    struct Job {
        batch: Arc<Batch>,
        paths: Vec<PathBuf>,
    }

    fn show_error(app: &AppHandle, message: String) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let _ = crate::ball::ball_message(handle, format!("导入失败：{message}"));
        });
    }

    // 同一拖拽的文件承诺可以分别异步完成；串行交给现有导入入口，避免互相
    // 触发“正在导入另一批”的拒绝。Job 持有 Batch 直到导入结束，临时文件不会提前删除。
    fn queue() -> Option<&'static mpsc::Sender<Job>> {
        static QUEUE: OnceLock<Option<mpsc::Sender<Job>>> = OnceLock::new();
        QUEUE
            .get_or_init(|| {
                let (sender, receiver) = mpsc::channel::<Job>();
                std::thread::Builder::new()
                    .name("macos-sticker-drop".into())
                    .spawn(move || {
                        for job in receiver {
                            let app = job.batch.app.clone();
                            let result = tauri::async_runtime::block_on(crate::imports::run(
                                app.clone(),
                                job.paths,
                                true,
                            ));
                            match result {
                                Ok(report) => {
                                    let _ = app.emit_to("ball", "external-drop-result", report);
                                }
                                Err(error) => show_error(&app, error),
                            }
                            drop(job.batch);
                        }
                    })
                    .ok()
                    .map(|_| sender)
            })
            .as_ref()
    }

    /// # Safety
    /// `app` 必须是关联对象持有的有效 AppHandle 指针，且每个指针只释放一次。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn sticker_macos_drop_free_app(app: *mut c_void) {
        if !app.is_null() {
            unsafe { drop(Box::from_raw(app.cast::<AppHandle>())) };
        }
    }

    /// # Safety
    /// `app` 必须指向在整个调用期间存活的 AppHandle，调用必须发生在主线程。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn sticker_macos_drop_active(app: *mut c_void, active: bool) {
        if let Some(app) = unsafe { app.cast::<AppHandle>().as_ref() } {
            let _ = crate::ball::external_drop(app, active);
        }
    }

    /// # Safety
    /// `app` 必须是存活的 AppHandle。返回指针由 Objective-C 持有并释放一次。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn sticker_macos_drop_begin(app: *mut c_void) -> *mut c_void {
        let Some(app) = (unsafe { app.cast::<AppHandle>().as_ref() }) else {
            return std::ptr::null_mut();
        };
        let result = (|| {
            let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
            let destination =
                CString::new(temporary.path().as_os_str().as_bytes()).map_err(|e| e.to_string())?;
            Ok::<_, String>(Arc::new(Batch {
                app: app.clone(),
                temporary,
                destination,
                budget: Mutex::new((0, 0)),
            }))
        })();
        match result {
            Ok(batch) => Box::into_raw(Box::new(batch)).cast(),
            Err(error) => {
                show_error(app, error);
                std::ptr::null_mut()
            }
        }
    }

    /// # Safety
    /// `batch` 必须指向存活的 Batch；返回字符串的借用只在该 Batch 存活期间有效。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn sticker_macos_drop_directory(batch: *mut c_void) -> *const c_char {
        unsafe { batch.cast::<Arc<Batch>>().as_ref() }
            .map_or(std::ptr::null(), |batch| batch.destination.as_ptr())
    }

    /// # Safety
    /// `batch` 必须存活；`error` 若非空须指向以零结尾的字符串。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn sticker_macos_drop_error(batch: *mut c_void, error: *const c_char) {
        if let Some(batch) = unsafe { batch.cast::<Arc<Batch>>().as_ref() } {
            let message = if error.is_null() {
                "无法读取拖入的图片".into()
            } else {
                unsafe { CStr::from_ptr(error) }
                    .to_string_lossy()
                    .into_owned()
            };
            show_error(&batch.app, message);
        }
    }

    /// # Safety
    /// `batch` 必须存活；`paths` 若非空，必须包含 `count` 个有效的零结尾字符串指针。
    /// 文件名数组只在此调用期间有效；先复制所有路径，再将持有 Batch 的工作发送给队列。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn sticker_macos_drop_enqueue(
        batch: *mut c_void,
        paths: *const *const c_char,
        count: usize,
    ) {
        let Some(batch) = (unsafe { batch.cast::<Arc<Batch>>().as_ref() }) else {
            return;
        };
        if paths.is_null() || count == 0 || count > super::MAX_FILES {
            show_error(&batch.app, "拖入文件为空或超过 256 项".into());
            return;
        }
        let mut received = Vec::with_capacity(count);
        for path in unsafe { std::slice::from_raw_parts(paths, count) } {
            if path.is_null() {
                show_error(&batch.app, "拖入文件路径无效".into());
                return;
            }
            let bytes = unsafe { CStr::from_ptr(*path) }.to_bytes();
            received.push(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)));
        }
        let bytes = match validate_paths(batch.temporary.path(), &received) {
            Ok(bytes) => bytes,
            Err(error) => {
                show_error(&batch.app, error);
                return;
            }
        };
        let Ok(mut budget) = batch.budget.lock() else {
            show_error(&batch.app, "无法统计拖入文件".into());
            return;
        };
        let files = budget.0.saturating_add(received.len());
        let size = budget.1.saturating_add(bytes);
        if files > super::MAX_FILES || size > super::MAX_BYTES {
            show_error(&batch.app, "拖入文件超过 256 项或总量超过 100 MB".into());
            return;
        }
        *budget = (files, size);
        drop(budget);
        let job = Job {
            batch: Arc::clone(batch),
            paths: received,
        };
        if queue().is_none_or(|queue| queue.send(job).is_err()) {
            show_error(&batch.app, "无法启动拖入导入线程".into());
        }
    }

    /// # Safety
    /// `batch` 必须是本模块返回的指针，并且只释放一次。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn sticker_macos_drop_free_batch(batch: *mut c_void) {
        if !batch.is_null() {
            unsafe { drop(Box::from_raw(batch.cast::<Arc<Batch>>())) };
        }
    }

    pub fn install(window: &tauri::WebviewWindow) -> Result<(), String> {
        let app = window.app_handle().clone();
        window
            .with_webview(move |webview| {
                let app = Box::into_raw(Box::new(app)).cast::<c_void>();
                // SAFETY: with_webview 提供主线程上的 WKWebView；安装成功后关联对象
                // 拥有 AppHandle，失败时在这里释放。原有文件拖拽仍交给 Wry。
                if !unsafe { sticker_macos_install(webview.inner().cast(), app) } {
                    unsafe { sticker_macos_drop_free_app(app) };
                    eprintln!("无法安装 macOS 图片拖入处理器，文件拖入仍可使用");
                }
            })
            .map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "macos")]
pub use native::install;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn received_files_must_be_regular_files_in_the_batch_directory() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let good = root.path().join("received.png");
        let other = outside.path().join("other.png");
        std::fs::write(&good, b"image").unwrap();
        std::fs::write(&other, b"image").unwrap();
        assert!(validate_paths(root.path(), std::slice::from_ref(&good)).is_ok());
        assert!(validate_paths(root.path(), &[other]).is_err());
        assert!(validate_paths(root.path(), &[root.path().to_path_buf()]).is_err());
        assert!(validate_paths(root.path(), &[]).is_err());
        assert!(validate_paths(root.path(), &vec![good; MAX_FILES + 1]).is_err());
    }

    #[test]
    fn received_files_are_limited_by_total_bytes() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("large");
        std::fs::File::create(&file)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        assert!(validate_paths(root.path(), &[file]).is_err());
    }
}
