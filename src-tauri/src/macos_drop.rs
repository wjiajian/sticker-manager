//! Extend Wry's native drag destination without changing its class or file-path handling.
use crate::{
    clipboard::macos::{self, IMAGE_UTIS},
    diagnostics::drop_diag,
    drop_policy::{self, DragDecision},
    staging::{self, ExternalDrop, Origin, Staging, StagingRoot},
};
use objc2::{
    ClassType, exception, ffi,
    runtime::{AnyClass, AnyObject, Bool, Imp, ProtocolObject, Sel},
    sel,
};
use objc2_app_kit::{NSDragOperation, NSDraggingInfo, NSFilePromiseReceiver, NSView};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSOperationQueue, NSURL};
use std::{
    cell::RefCell,
    collections::HashMap,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, WebviewWindow};

#[derive(Clone, Copy)]
pub enum Surface {
    Ball,
    Main,
}
#[derive(Clone, Copy)]
enum Session {
    Idle,
    Wry(isize),
    Native(isize, NSDragOperation),
    Rejected(isize),
}
impl Session {
    fn seq(self) -> Option<isize> {
        match self {
            Self::Idle => None,
            Self::Wry(n) | Self::Native(n, _) | Self::Rejected(n) => Some(n),
        }
    }
}
struct ViewState {
    app: AppHandle,
    surface: Surface,
    session: Session,
    rejected: Option<isize>,
}
thread_local! { static VIEWS: RefCell<HashMap<usize, ViewState>> = RefCell::new(HashMap::new()); }
static ORIGINALS: OnceLock<[Option<Imp>; 6]> = OnceLock::new();
static INSTALL: OnceLock<Result<(), String>> = OnceLock::new();
type Info = ProtocolObject<dyn NSDraggingInfo>;
type OpImp = unsafe extern "C-unwind" fn(&AnyObject, Sel, &Info) -> NSDragOperation;
type BoolImp = unsafe extern "C-unwind" fn(&AnyObject, Sel, &Info) -> Bool;
type VoidImp = unsafe extern "C-unwind" fn(&AnyObject, Sel, &Info);
fn key(view: &AnyObject) -> usize {
    view as *const _ as usize
}
fn state(view: &AnyObject) -> Option<(AppHandle, Surface, Session)> {
    VIEWS.with_borrow(|views| {
        views
            .get(&key(view))
            .map(|v| (v.app.clone(), v.surface, v.session))
    })
}
fn set(view: &AnyObject, session: Session) {
    VIEWS.with_borrow_mut(|views| {
        if let Some(v) = views.get_mut(&key(view)) {
            v.session = session;
        }
    });
}
fn guarded<T>(f: impl FnOnce() -> T) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(|| exception::catch(AssertUnwindSafe(f)))) {
        Ok(Ok(value)) => Some(value),
        _ => {
            eprintln!("原生拖放处理失败，退回 Wry");
            None
        }
    }
}
fn original(index: usize) -> Option<Imp> {
    ORIGINALS.get().and_then(|imps| imps[index])
}
unsafe fn original_op(index: usize, view: &AnyObject, cmd: Sel, info: &Info) -> NSDragOperation {
    original(index).map_or(NSDragOperation::empty(), |imp| unsafe {
        std::mem::transmute::<Imp, OpImp>(imp)(view, cmd, info)
    })
}
unsafe fn original_bool(index: usize, view: &AnyObject, cmd: Sel, info: &Info) -> Bool {
    original(index).map_or(Bool::YES, |imp| unsafe {
        std::mem::transmute::<Imp, BoolImp>(imp)(view, cmd, info)
    })
}
unsafe fn original_void(index: usize, view: &AnyObject, cmd: Sel, info: &Info) {
    if let Some(imp) = original(index) {
        unsafe { std::mem::transmute::<Imp, VoidImp>(imp)(view, cmd, info) };
    }
}
fn hover(app: &AppHandle, surface: Surface, active: bool) {
    match surface {
        Surface::Ball => {
            let _ = crate::ball::external_drop(app, active);
        }
        Surface::Main => staging::emit(
            app,
            if active {
                ExternalDrop::Enter
            } else {
                ExternalDrop::Leave
            },
        ),
    }
}
fn entered(view: &AnyObject, info: &Info) -> Option<NSDragOperation> {
    let (app, surface, previous) = state(view)?;
    if matches!(previous, Session::Native(..)) {
        hover(&app, surface, false);
    }
    let board = info.draggingPasteboard();
    let seq = info.draggingSequenceNumber();
    let mask = info.draggingSourceOperationMask();
    let op = if mask.contains(NSDragOperation::Copy) {
        NSDragOperation::Copy
    } else {
        mask & NSDragOperation::Generic
    };
    let scan = macos::files(&board);
    let image = board
        .availableTypeFromArray(&macos::types(IMAGE_UTIS))
        .is_some();
    let promise = board
        .availableTypeFromArray(&NSFilePromiseReceiver::readableDraggedTypes())
        .is_some();
    // Prefer native image/promise over a TCC-unreadable container file URL (e.g. macOS WeChat),
    // while still forwarding unreadable-only file drags to Wry so the permission error surfaces.
    let files = !scan.readable.is_empty() || (scan.unreadable && !image && !promise);
    let decision = drop_policy::classify(files, image, promise, !op.is_empty());
    drop_diag!("drag decision={decision:?} sequence={seq}");
    if crate::diagnostics::enabled()
        && let Some(types) = board.types()
    {
        for t in types.iter() {
            drop_diag!("drag type={t}");
        }
    }
    match decision {
        DragDecision::Wry => {
            set(view, Session::Wry(seq));
            None
        }
        DragDecision::Native => {
            set(view, Session::Native(seq, op));
            hover(&app, surface, true);
            Some(op)
        }
        DragDecision::Rejected(_) => {
            set(view, Session::Rejected(seq));
            let notify = info.draggingSource().is_none()
                && VIEWS.with_borrow_mut(|views| {
                    views
                        .get_mut(&key(view))
                        .is_some_and(|v| v.rejected.replace(seq) != Some(seq))
                });
            if notify {
                match surface {
                    Surface::Ball => crate::ball::notify(&app, staging::UNSUPPORTED_DROP.into()),
                    Surface::Main => staging::emit(
                        &app,
                        ExternalDrop::Rejected {
                            message: staging::UNSUPPORTED_DROP.into(),
                        },
                    ),
                }
            }
            Some(NSDragOperation::empty())
        }
    }
}
unsafe extern "C-unwind" fn on_enter(view: &AnyObject, cmd: Sel, info: &Info) -> NSDragOperation {
    guarded(|| entered(view, info))
        .flatten()
        .unwrap_or_else(|| unsafe { original_op(0, view, cmd, info) })
}
unsafe extern "C-unwind" fn on_update(view: &AnyObject, cmd: Sel, info: &Info) -> NSDragOperation {
    guarded(|| {
        let (_, _, session) = state(view)?;
        if session.seq() != Some(info.draggingSequenceNumber()) {
            return entered(view, info)
                .or_else(|| Some(unsafe { original_op(0, view, sel!(draggingEntered:), info) }));
        }
        match session {
            Session::Native(_, op) => Some(op),
            Session::Rejected(_) => Some(NSDragOperation::empty()),
            _ => None,
        }
    })
    .flatten()
    .unwrap_or_else(|| unsafe { original_op(1, view, cmd, info) })
}
unsafe extern "C-unwind" fn on_exit(view: &AnyObject, cmd: Sel, info: &Info) {
    if guarded(|| {
        let Some((app, surface, session)) = state(view) else {
            return false;
        };
        set(view, Session::Idle);
        if matches!(session, Session::Native(..)) {
            hover(&app, surface, false);
        }
        !matches!(session, Session::Wry(_) | Session::Idle)
    }) != Some(true)
    {
        unsafe { original_void(2, view, cmd, info) };
    }
}
unsafe extern "C-unwind" fn on_prepare(view: &AnyObject, cmd: Sel, info: &Info) -> Bool {
    guarded(|| match state(view)?.2 {
        Session::Native(..) => Some(Bool::YES),
        Session::Rejected(_) => Some(Bool::NO),
        _ => None,
    })
    .flatten()
    .unwrap_or_else(|| unsafe { original_bool(3, view, cmd, info) })
}
unsafe extern "C-unwind" fn on_perform(view: &AnyObject, cmd: Sel, info: &Info) -> Bool {
    guarded(|| {
        let (app, surface, session) = state(view)?;
        match session {
            Session::Native(..) => {
                hover(&app, surface, false);
                let board = info.draggingPasteboard();
                receive(app, surface, &board);
                Some(Bool::YES)
            }
            Session::Rejected(_) => Some(Bool::NO),
            _ => None,
        }
    })
    .flatten()
    .unwrap_or_else(|| unsafe { original_bool(4, view, cmd, info) })
}
unsafe extern "C-unwind" fn on_conclude(view: &AnyObject, cmd: Sel, info: &Info) {
    if guarded(|| {
        let Some((_, _, session)) = state(view) else {
            return false;
        };
        set(view, Session::Idle);
        matches!(session, Session::Native(..) | Session::Rejected(_))
    }) != Some(true)
    {
        unsafe { original_void(5, view, cmd, info) };
    }
}
fn receive(app: AppHandle, surface: Surface, board: &objc2_app_kit::NSPasteboard) {
    let root = app.state::<StagingRoot>().0.clone();
    let ball = matches!(surface, Surface::Ball);
    let raw = macos::read_raw(board, IMAGE_UTIS);
    let staging = Staging::new(&root, Origin::Drop);
    let mut staging = match staging {
        Ok(s) => s,
        Err(error) => {
            tauri::async_runtime::spawn(staging::complete(app, Err(error), ball));
            return;
        }
    };
    if !raw.has_animation() {
        let classes = NSArray::<AnyClass>::from_slice(&[NSFilePromiseReceiver::class()]);
        let receivers = unsafe { board.readObjectsForClasses_options(&classes, None) };
        let receivers: Vec<_> = receivers
            .iter()
            .flat_map(|r| r.iter())
            .filter_map(|obj| obj.downcast::<NSFilePromiseReceiver>().ok())
            .collect();
        if !receivers.is_empty() {
            if ball {
                crate::ball::notify(&app, "正在接收拖入的图片…".into());
            } else {
                staging::emit(&app, ExternalDrop::Receiving);
            }
            let (send, recv) = std::sync::mpsc::channel::<Result<PathBuf, String>>();
            let finished = Arc::new(AtomicBool::new(false));
            let queue = NSOperationQueue::new();
            queue.setMaxConcurrentOperationCount(2);
            let mut expected = 0;
            if receivers.len() > staging::MAX_ITEMS {
                staging.fail(
                    "承诺文件",
                    "图片超过 256 项",
                    receivers.len() - staging::MAX_ITEMS,
                );
            }
            for (index, receiver) in receivers.iter().take(staging::MAX_ITEMS).enumerate() {
                // AppKit may return nil for -[NSFilePromiseReceiver fileNames] before receiving
                // completes, which panics in objc2-app-kit's non-null wrapper.
                let file_names: Option<objc2::rc::Retained<NSArray<objc2_foundation::NSString>>> =
                    unsafe { objc2::msg_send![receiver, fileNames] };
                let count = file_names.map_or(1, |names| names.len().max(1));
                if expected + count > staging::MAX_ITEMS {
                    staging.fail("承诺文件", "图片超过 256 项", count);
                    continue;
                }
                let dir = staging.path().join("promised").join(index.to_string());
                if let Err(error) = std::fs::create_dir_all(&dir) {
                    staging.fail("承诺文件", error.to_string(), count);
                    continue;
                }
                let Some(path) = dir.to_str() else {
                    staging.fail("承诺文件", "暂存路径无效", count);
                    continue;
                };
                let destination =
                    NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(path));
                let send = send.clone();
                let callback_finished = finished.clone();
                let block = block2::RcBlock::new(
                    move |url: std::ptr::NonNull<NSURL>, error: *mut NSError| {
                        if callback_finished.load(Ordering::Acquire) {
                            return;
                        }
                        let result = guarded(|| {
                            if let Some(error) = unsafe { error.as_ref() } {
                                drop_diag!(
                                    "promise error domain={} code={}",
                                    error.domain(),
                                    error.code()
                                );
                                return Err("无法接收承诺文件".into());
                            }
                            unsafe { url.as_ref() }
                                .path()
                                .map(|p| PathBuf::from(p.to_string()))
                                .ok_or_else(|| "承诺文件路径无效".into())
                        })
                        .unwrap_or_else(|| Err("无法接收承诺文件".into()));
                        let _ = send.send(result);
                    },
                );
                if guarded(|| unsafe {
                    receiver.receivePromisedFilesAtDestination_options_operationQueue_reader(
                        &destination,
                        &NSDictionary::new(),
                        &queue,
                        &block,
                    )
                })
                .is_some()
                {
                    expected += count;
                } else {
                    staging.fail("承诺文件", "无法接收承诺文件", count);
                }
            }
            drop(send);
            tauri::async_runtime::spawn(async move {
                let staged = tauri::async_runtime::spawn_blocking(move || {
                    let mut accepted = 0;
                    let start = Instant::now();
                    for index in 0..expected {
                        match recv
                            .recv_timeout(Duration::from_secs(30).saturating_sub(start.elapsed()))
                        {
                            Ok(Ok(path)) => match staging.adopt(&path) {
                                Ok(()) => accepted += 1,
                                Err(e) => staging.fail("承诺文件", e, 1),
                            },
                            Ok(Err(e)) => staging.fail("承诺文件", e, 1),
                            Err(_) => {
                                staging.fail("承诺文件", "接收拖入的图片超时", expected - index);
                                break;
                            }
                        }
                    }
                    // One consumer owns the batch; late callbacks are ignored after timeout/completion.
                    let _ = finished.swap(true, Ordering::AcqRel);
                    if accepted == 0 {
                        macos::stage_raw(&mut staging, raw);
                    }
                    staging.finish()
                })
                .await
                .map_err(|e| e.to_string());
                staging::complete(app, staged, ball).await;
            });
            return;
        }
    }
    tauri::async_runtime::spawn(async move {
        let staged = tauri::async_runtime::spawn_blocking(move || {
            if raw.is_empty() && !raw.has_failures() {
                staging.fail("图片", staging::UNSUPPORTED_DROP, 1);
            }
            macos::stage_raw(&mut staging, raw);
            staging.finish()
        })
        .await
        .map_err(|e| e.to_string());
        staging::complete(app, staged, ball).await;
    });
}
fn replace(cls: &AnyClass) -> Result<(), String> {
    let methods: [(Sel, Imp, &[u8], &std::ffi::CStr); 6] = unsafe {
        [
            (
                sel!(draggingEntered:),
                std::mem::transmute::<OpImp, Imp>(on_enter),
                b"Q",
                c"Q@:@",
            ),
            (
                sel!(draggingUpdated:),
                std::mem::transmute::<OpImp, Imp>(on_update),
                b"Q",
                c"Q@:@",
            ),
            (
                sel!(draggingExited:),
                std::mem::transmute::<VoidImp, Imp>(on_exit),
                b"v",
                c"v@:@",
            ),
            (
                sel!(prepareForDragOperation:),
                std::mem::transmute::<BoolImp, Imp>(on_prepare),
                b"Bc",
                c"B@:@",
            ),
            (
                sel!(performDragOperation:),
                std::mem::transmute::<BoolImp, Imp>(on_perform),
                b"Bc",
                c"B@:@",
            ),
            (
                sel!(concludeDragOperation:),
                std::mem::transmute::<VoidImp, Imp>(on_conclude),
                b"v",
                c"v@:@",
            ),
        ]
    };
    let mut originals = [None; 6];
    for (i, (sel, _, ret, _)) in methods.iter().enumerate() {
        if let Some(method) = cls.instance_method(*sel) {
            if method.arguments_count() != 3
                || !method
                    .return_type()
                    .to_bytes()
                    .first()
                    .is_some_and(|r| ret.contains(r))
                || method
                    .argument_type(2)
                    .is_none_or(|arg| !arg.to_bytes().starts_with(b"@"))
            {
                return Err(format!("Wry 拖放方法签名不匹配：{sel:?}"));
            }
            originals[i] = Some(method.implementation());
        } else if !matches!(i, 3 | 5) {
            return Err(format!("Wry 拖放方法不存在：{sel:?}"));
        }
    }
    let _ = ORIGINALS.set(originals);
    for (sel, imp, _, encoding) in methods {
        let own = cls
            .instance_methods()
            .iter()
            .copied()
            .find(|m| m.name() == sel);
        if let Some(method) = own {
            unsafe {
                ffi::method_setImplementation(method, imp);
            }
        } else {
            let encoding = cls.instance_method(sel).map_or(encoding, |m| {
                let ptr = unsafe { ffi::method_getTypeEncoding(m) };
                if ptr.is_null() {
                    encoding
                } else {
                    unsafe { std::ffi::CStr::from_ptr(ptr) }
                }
            });
            if !unsafe {
                ffi::class_addMethod(
                    std::ptr::from_ref(cls).cast_mut(),
                    sel,
                    imp,
                    encoding.as_ptr(),
                )
            }
            .as_bool()
            {
                return Err(format!("无法安装拖放方法：{sel:?}"));
            }
        }
    }
    Ok(())
}
fn is_wry_webview_name(name: &str) -> bool {
    name == "WryWebView"
        || name
            .strip_prefix("wry::")
            .and_then(|rest| rest.rsplit("::").next())
            .and_then(|tail| tail.strip_prefix("WryWebView"))
            .is_some_and(|ver| ver.is_empty() || ver.starts_with(|c: char| c.is_ascii_digit()))
}
fn find_wry_webview(mut cls: &AnyClass) -> Option<&AnyClass> {
    loop {
        if cls.name().to_str().is_ok_and(is_wry_webview_name) {
            return Some(cls);
        }
        cls = cls.superclass()?;
    }
}
pub fn install(window: &WebviewWindow, surface: Surface) -> Result<(), String> {
    let app = window.app_handle().clone();
    let window_handle = window.clone();
    window
        .with_webview(move |webview| {
            let result = guarded(|| {
                let object = unsafe { &*webview.inner().cast::<AnyObject>() };
                let cls = find_wry_webview(object.class()).ok_or("未找到 WryWebView")?;
                INSTALL.get_or_init(|| replace(cls)).clone()?;
                let view = unsafe { &*webview.inner().cast::<NSView>() };
                let mut registered = view.registeredDraggedTypes().to_vec();
                registered.extend(macos::types(IMAGE_UTIS).to_vec());
                registered.extend(NSFilePromiseReceiver::readableDraggedTypes().to_vec());
                view.registerForDraggedTypes(&NSArray::from_retained_slice(&registered));
                let id = key(object);
                VIEWS.with_borrow_mut(|views| {
                    views.insert(
                        id,
                        ViewState {
                            app,
                            surface,
                            session: Session::Idle,
                            rejected: None,
                        },
                    );
                });
                window_handle.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::Destroyed) {
                        VIEWS.with_borrow_mut(|views| {
                            views.remove(&id);
                        });
                    }
                });
                Ok::<_, String>(())
            });
            if let Some(Err(error)) = result {
                eprintln!("原生拖放安装失败：{error}");
            }
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_foundation::NSObject;

    #[test]
    fn matches_legacy_and_versioned_wry_webview_class_names() {
        assert!(is_wry_webview_name("WryWebView"));
        assert!(is_wry_webview_name(
            "wry::wkwebview::class::wry_web_view::WryWebView0.57.1"
        ));
        assert!(is_wry_webview_name(
            "wry::wkwebview::class::wry_web_view::WryWebView"
        ));
        assert!(!is_wry_webview_name("WryWebViewParent"));
        assert!(!is_wry_webview_name(
            "wry::wkwebview::class::wry_web_view_delegate::WryWebViewDelegate0.57.1"
        ));
        assert!(!is_wry_webview_name("WKWebView"));
        assert!(!is_wry_webview_name("NSKVONotifying_WryWebView"));
    }

    #[test]
    fn missing_wry_webview_superclass_returns_none() {
        assert!(find_wry_webview(NSObject::class()).is_none());
    }
}
