//! Windows 聊天软件可能交出虚拟文件或图片数据，而不是磁盘路径。
use std::{cell::Cell, ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
use tauri::{Emitter, Manager};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HANDLE, HWND, LPARAM, POINTL, S_OK},
        System::{
            Com::{
                COINIT_MULTITHREADED, COWAIT_DISPATCH_CALLS, COWAIT_DISPATCH_WINDOW_MESSAGES,
                CoInitializeEx, CoUninitialize, CoWaitForMultipleHandles, DVASPECT_CONTENT,
                FORMATETC, IDataObject, STGMEDIUM, TYMED_HGLOBAL, TYMED_ISTREAM,
            },
            DataExchange::RegisterClipboardFormatW,
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{
                DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, IDropTarget, IDropTarget_Impl,
                RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop,
            },
            SystemServices::MODIFIERKEYS_FLAGS,
        },
        UI::{
            Shell::{DragQueryFileW, FILEDESCRIPTORW, HDROP, IDataObjectAsyncCapability},
            WindowsAndMessaging::EnumChildWindows,
        },
    },
    core::{AgileReference, BOOL, HSTRING, Interface, Ref, implement},
};

const MAX_BYTES: usize = 100 * 1024 * 1024;
const MAX_FILES: usize = 256;

fn registered(name: &str) -> u16 {
    // SAFETY: HSTRING 在调用期间持有有效的 UTF-16 字符串。
    unsafe { RegisterClipboardFormatW(&HSTRING::from(name)) as u16 }
}

fn format(id: u16, index: i32, medium: u32) -> FORMATETC {
    FORMATETC {
        cfFormat: id,
        dwAspect: DVASPECT_CONTENT.0,
        lindex: index,
        tymed: medium,
        ..Default::default()
    }
}

struct Medium(STGMEDIUM);
impl Drop for Medium {
    fn drop(&mut self) {
        // SAFETY: 每份成功 GetData 的介质只释放一次，包括源提供的释放对象。
        unsafe { ReleaseStgMedium(&mut self.0) };
    }
}

fn get(data: &IDataObject, request: &FORMATETC) -> Result<Medium, String> {
    unsafe { data.GetData(request) }
        .map(Medium)
        .map_err(|e| e.to_string())
}

fn bytes(medium: &Medium) -> Result<Vec<u8>, String> {
    // SAFETY: 按 tymed 读取 union，拷贝期间介质始终存活。
    unsafe {
        if medium.0.tymed == TYMED_HGLOBAL.0 as u32 {
            let handle = medium.0.u.hGlobal;
            let size = GlobalSize(handle);
            if size == 0 || size > MAX_BYTES {
                return Err("拖入数据为空或超过 100 MB".into());
            }
            let pointer = GlobalLock(handle);
            if pointer.is_null() {
                return Err("无法读取拖入数据".into());
            }
            let result = std::slice::from_raw_parts(pointer.cast::<u8>(), size).to_vec();
            let _ = GlobalUnlock(handle);
            return Ok(result);
        }
        if medium.0.tymed == TYMED_ISTREAM.0 as u32 {
            let stream = medium.0.u.pstm.as_ref().ok_or("拖入文件流为空")?;
            let mut result = Vec::new();
            let mut buffer = [0u8; 65536];
            loop {
                let mut read = 0;
                stream
                    .Read(
                        buffer.as_mut_ptr().cast(),
                        buffer.len() as u32,
                        Some(&mut read),
                    )
                    .ok()
                    .map_err(|e| e.to_string())?;
                if read as usize > buffer.len() || result.len() + read as usize > MAX_BYTES {
                    return Err("拖入文件超过 100 MB".into());
                }
                result.extend_from_slice(&buffer[..read as usize]);
                if read == 0 {
                    return Ok(result);
                }
            }
        }
    }
    Err("不支持的拖入数据介质".into())
}

fn descriptors(bytes: &[u8]) -> Result<Vec<String>, String> {
    let count = bytes.get(..4).ok_or("虚拟文件列表不完整")?;
    let count = u32::from_le_bytes(count.try_into().unwrap()) as usize;
    let size = std::mem::size_of::<FILEDESCRIPTORW>();
    if count == 0 || count > MAX_FILES || bytes.len() < 4 + count * size {
        return Err("虚拟文件列表无效或超过 256 项".into());
    }
    (0..count)
        .map(|index| {
            // SAFETY: 已检查每个描述符的完整边界，外部缓冲区无需对齐。
            let descriptor = unsafe {
                bytes[4 + index * size..]
                    .as_ptr()
                    .cast::<FILEDESCRIPTORW>()
                    .read_unaligned()
            };
            let filename = descriptor.cFileName;
            let end = filename.iter().position(|c| *c == 0).unwrap_or(260);
            let name = String::from_utf16(&filename[..end]).map_err(|_| "虚拟文件名编码无效")?;
            // 每个项目写入独立子目录，只保留文件名，禁止外部提供目录穿越。
            let name = name.rsplit(['/', '\\']).next().unwrap_or("");
            if name.is_empty() || name == "." || name == ".." || name.contains(':') {
                return Err("虚拟文件名无效".into());
            }
            Ok(name.to_owned())
        })
        .collect()
}

fn dib_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < 40 {
        return Err("位图数据不完整".into());
    }
    let read_u32 =
        |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    let header = read_u32(0);
    if ![40, 108, 124].contains(&header) || bytes.len() < header {
        return Err("不支持的位图头".into());
    }
    let bits = u16::from_le_bytes(bytes[14..16].try_into().unwrap());
    let colors = read_u32(32);
    let palette = if colors != 0 {
        colors
    } else if bits <= 8 {
        1usize << bits
    } else {
        0
    };
    let masks = if header == 40 {
        match read_u32(16) {
            3 => 12,
            6 => 16,
            _ => 0,
        }
    } else {
        0
    };
    let offset = header
        .checked_add(masks)
        .and_then(|n| palette.checked_mul(4).and_then(|p| n.checked_add(p)))
        .filter(|n| *n <= bytes.len())
        .ok_or("位图调色板无效")?;
    let mut bmp = Vec::with_capacity(bytes.len() + 14);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((bytes.len() + 14) as u32).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&((offset + 14) as u32).to_le_bytes());
    bmp.extend_from_slice(bytes);
    let image = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp)
        .map_err(|e| e.to_string())?;
    let mut png = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(png.into_inner())
}

struct Received {
    paths: Vec<PathBuf>,
    _temporary: Option<tempfile::TempDir>,
}

struct ComApartment;
impl ComApartment {
    fn enter() -> windows::core::Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

// AgileReference 持有对象并通过 COM 封送；绝不把 apartment-bound IDataObject 裸指针传给线程。
fn receive_in_background(
    data: &IDataObject,
    complete: impl FnOnce(Result<Received, String>) + Send + 'static,
) -> Result<(), String> {
    let reference = AgileReference::new(data).map_err(|e| e.to_string())?;
    let asynchronous = data
        .cast::<IDataObjectAsyncCapability>()
        .ok()
        .filter(|operation| unsafe { operation.GetAsyncMode().is_ok_and(|mode| mode.as_bool()) });
    let operation_reference = asynchronous
        .as_ref()
        .map(AgileReference::new)
        .transpose()
        .map_err(|e| e.to_string())?;
    if let Some(operation) = &asynchronous {
        unsafe {
            operation.StartOperation(None).map_err(|e| e.to_string())?;
        }
    }
    let is_async = asynchronous.is_some();
    let worker = std::thread::Builder::new()
        .name("sticker-drop".into())
        .spawn(move || {
            let result = (|| {
                let _apartment = ComApartment::enter().map_err(|e| e.to_string())?;
                // 局部绑定保证错误返回时封送引用也先于 apartment 释放。
                let reference = reference;
                let operation_reference = operation_reference;
                let operation = operation_reference
                    .as_ref()
                    .map(AgileReference::resolve)
                    .transpose()
                    .map_err(|e| e.to_string())?;
                let result = reference
                    .resolve()
                    .map_err(|e| e.to_string())
                    .and_then(|data| receive(&data));
                if let Some(operation) = operation {
                    unsafe {
                        operation
                            .EndOperation(
                                if result.is_ok() { S_OK } else { E_FAIL },
                                None,
                                if result.is_ok() { DROPEFFECT_COPY.0 } else { 0 },
                            )
                            .map_err(|e| e.to_string())?;
                    }
                }
                // reference 也必须在 CoUninitialize 之前释放。
                drop(reference);
                result
            })();
            complete(result);
        });
    let worker = match worker {
        Ok(worker) => worker,
        Err(error) => {
            if let Some(operation) = &asynchronous {
                let _ = unsafe { operation.EndOperation(E_FAIL, None, 0) };
            }
            return Err(error.to_string());
        }
    };
    if !is_async {
        // 旧源未协商异步时，必须保持 Drop 生命周期。OLE modal wait 继续派发
        // COM 与窗口消息；数据获取和解码仍在线程上运行，窗口不因慢源停止响应。
        use std::os::windows::io::AsRawHandle;
        unsafe {
            CoWaitForMultipleHandles(
                (COWAIT_DISPATCH_CALLS.0 | COWAIT_DISPATCH_WINDOW_MESSAGES.0) as u32,
                u32::MAX,
                &[HANDLE(worker.as_raw_handle())],
            )
            .map_err(|e| e.to_string())?;
        }
        worker.join().map_err(|_| "读取拖入数据的线程异常退出")?;
    }
    Ok(())
}

fn receive(data: &IDataObject) -> Result<Received, String> {
    if let Ok(medium) = get(data, &format(15, -1, TYMED_HGLOBAL.0 as u32)) {
        let drop = unsafe { HDROP(medium.0.u.hGlobal.0) };
        let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
        if count == 0 || count as usize > MAX_FILES {
            return Err("拖入文件为空或超过 256 项".into());
        }
        let mut paths = Vec::new();
        for index in 0..count {
            let length = unsafe { DragQueryFileW(drop, index, None) };
            let mut name = vec![0u16; length as usize + 1];
            unsafe {
                DragQueryFileW(drop, index, Some(&mut name));
            }
            paths.push(PathBuf::from(OsString::from_wide(&name[..length as usize])));
        }
        return Ok(Received {
            paths,
            _temporary: None,
        });
    }
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    if let Ok(medium) = get(
        data,
        &format(
            registered("FileGroupDescriptorW"),
            -1,
            TYMED_HGLOBAL.0 as u32,
        ),
    ) {
        let names = descriptors(&bytes(&medium)?)?;
        let mut paths = Vec::new();
        let mut total = 0;
        for (index, name) in names.iter().enumerate() {
            let medium = get(
                data,
                &format(
                    registered("FileContents"),
                    index as i32,
                    (TYMED_HGLOBAL.0 | TYMED_ISTREAM.0) as u32,
                ),
            )?;
            let content = bytes(&medium)?;
            total += content.len();
            if total > MAX_BYTES {
                return Err("拖入文件总量超过 100 MB".into());
            }
            let directory = temporary.path().join(index.to_string());
            std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
            let path = directory.join(name);
            std::fs::write(&path, content).map_err(|e| e.to_string())?;
            paths.push(path);
        }
        return Ok(Received {
            paths,
            _temporary: Some(temporary),
        });
    }
    for id in [registered("PNG"), 17, 8] {
        if let Ok(medium) = get(data, &format(id, -1, TYMED_HGLOBAL.0 as u32)) {
            let content = bytes(&medium)?;
            let content = if id == 17 || id == 8 {
                dib_png(&content)?
            } else {
                content
            };
            let path = temporary.path().join("收到的表情.png");
            std::fs::write(&path, content).map_err(|e| e.to_string())?;
            return Ok(Received {
                paths: vec![path],
                _temporary: Some(temporary),
            });
        }
    }
    Err("这个拖入来源没有提供可读取的图片或文件，请先保存图片后导入".into())
}

#[implement(IDropTarget)]
struct Target {
    app: tauri::AppHandle,
    accepted: Cell<bool>,
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        _: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let accepted = data.as_ref().is_some_and(|data| {
            [
                15,
                registered("FileGroupDescriptorW"),
                registered("PNG"),
                17,
                8,
            ]
            .iter()
            .any(|id| unsafe {
                data.QueryGetData(&format(*id, -1, TYMED_HGLOBAL.0 as u32))
                    .is_ok()
            })
        });
        self.accepted.set(accepted);
        unsafe {
            *effect = if accepted {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            };
        }
        let _ = crate::ball::external_drop(&self.app, accepted);
        if !accepted {
            let _ = crate::ball::ball_message(self.app.clone(), "拖入来源未提供图片文件".into());
        }
        Ok(())
    }
    fn DragOver(
        &self,
        _: MODIFIERKEYS_FLAGS,
        _: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe {
            *effect = if self.accepted.get() {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            };
        }
        Ok(())
    }
    fn DragLeave(&self) -> windows::core::Result<()> {
        self.accepted.set(false);
        let _ = crate::ball::external_drop(&self.app, false);
        Ok(())
    }
    fn Drop(
        &self,
        data: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        _: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let _ = self.DragLeave();
        unsafe {
            *effect = DROPEFFECT_NONE;
        }
        let result = data
            .as_ref()
            .ok_or_else(|| "拖入数据为空".into())
            .and_then(|data| {
                let app = self.app.clone();
                receive_in_background(data, move |received| {
                    tauri::async_runtime::spawn(async move {
                        let result = match &received {
                            Ok(received) => {
                                crate::imports::run(app.clone(), received.paths.clone(), true).await
                            }
                            Err(error) => Err(error.clone()),
                        };
                        match result {
                            Ok(report) => {
                                let _ = app.emit_to("ball", "external-drop-result", report);
                            }
                            Err(error) => {
                                let handle = app.clone();
                                let _ = app.run_on_main_thread(move || {
                                    let _ = crate::ball::ball_message(
                                        handle,
                                        format!("导入失败：{error}"),
                                    );
                                });
                            }
                        }
                        drop(received);
                    });
                })
            });
        match result {
            Ok(()) => unsafe {
                *effect = DROPEFFECT_COPY;
            },
            Err(error) => {
                let _ = crate::ball::ball_message(self.app.clone(), error);
            }
        }
        Ok(())
    }
}

pub fn install(window: &tauri::WebviewWindow) -> Result<(), String> {
    let root = window.hwnd().map_err(|e| e.to_string())?;
    let mut handles = vec![root];
    unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> BOOL {
        // SAFETY: EnumChildWindows 同步调用，参数指向本次安装拥有的 Vec。
        unsafe {
            (&mut *(parameter.0 as *mut Vec<HWND>)).push(window);
        }
        BOOL(1)
    }
    unsafe {
        let _ = EnumChildWindows(
            Some(root),
            Some(collect),
            LPARAM(&mut handles as *mut _ as isize),
        );
    }
    let target: IDropTarget = Target {
        app: window.app_handle().clone(),
        accepted: Cell::new(false),
    }
    .into();
    let mut registered = Vec::new();
    for handle in handles {
        unsafe {
            let _ = RevokeDragDrop(handle);
            if let Err(error) = RegisterDragDrop(handle, &target) {
                for handle in registered {
                    let _ = RevokeDragDrop(HWND(handle as _));
                }
                return Err(error.to_string());
            }
        }
        registered.push(handle.0 as isize);
    }
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            for handle in &registered {
                unsafe {
                    let _ = RevokeDragDrop(HWND(*handle as _));
                }
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::{
        Foundation::E_NOTIMPL,
        System::{
            Com::{IAdviseSink, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA},
            Memory::{GMEM_MOVEABLE, GlobalAlloc},
        },
    };

    #[derive(Default)]
    #[implement(IDataObject, IDataObjectAsyncCapability)]
    struct Sample {
        formats: Vec<(u16, i32, Vec<u8>, u32)>,
        async_state: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
        release_data: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    }

    #[allow(non_snake_case)]
    impl windows::Win32::UI::Shell::IDataObjectAsyncCapability_Impl for Sample_Impl {
        fn SetAsyncMode(&self, _: BOOL) -> windows::core::Result<()> {
            Ok(())
        }
        fn GetAsyncMode(&self) -> windows::core::Result<BOOL> {
            Ok(BOOL(self.async_state.is_some() as i32))
        }
        fn StartOperation(
            &self,
            _: Ref<windows::Win32::System::Com::IBindCtx>,
        ) -> windows::core::Result<()> {
            self.async_state
                .as_ref()
                .unwrap()
                .store(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn InOperation(&self) -> windows::core::Result<BOOL> {
            Ok(BOOL(
                self.async_state
                    .as_ref()
                    .is_some_and(|s| s.load(std::sync::atomic::Ordering::SeqCst) == 1)
                    as i32,
            ))
        }
        fn EndOperation(
            &self,
            result: windows::core::HRESULT,
            _: Ref<windows::Win32::System::Com::IBindCtx>,
            _: u32,
        ) -> windows::core::Result<()> {
            self.async_state.as_ref().unwrap().store(
                if result.is_ok() { 2 } else { 3 },
                std::sync::atomic::Ordering::SeqCst,
            );
            Ok(())
        }
    }

    #[allow(non_snake_case)]
    impl IDataObject_Impl for Sample_Impl {
        fn GetData(&self, request: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
            let request = unsafe { &*request };
            if request.cfFormat == registered("FileContents") {
                if let Some(release) = &self.release_data {
                    let start = std::time::Instant::now();
                    while !release.load(std::sync::atomic::Ordering::SeqCst) {
                        if start.elapsed() > std::time::Duration::from_secs(3) {
                            return Err(E_FAIL.into());
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
            }
            let (_, _, bytes, medium) = self
                .formats
                .iter()
                .find(|(id, index, _, medium)| {
                    *id == request.cfFormat
                        && *index == request.lindex
                        && request.tymed & medium != 0
                })
                .ok_or_else(|| windows::core::Error::from_hresult(E_NOTIMPL))?;
            unsafe {
                if *medium == TYMED_ISTREAM.0 as u32 {
                    let stream = windows::Win32::UI::Shell::SHCreateMemStream(Some(bytes))
                        .ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))?;
                    let mut result = STGMEDIUM {
                        tymed: *medium,
                        ..Default::default()
                    };
                    result.u.pstm = std::mem::ManuallyDrop::new(Some(stream));
                    return Ok(result);
                }
                let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
                let pointer = GlobalLock(handle);
                assert!(!pointer.is_null());
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len());
                let _ = GlobalUnlock(handle);
                let mut result = STGMEDIUM {
                    tymed: TYMED_HGLOBAL.0 as u32,
                    ..Default::default()
                };
                result.u.hGlobal = handle;
                Ok(result)
            }
        }
        fn QueryGetData(&self, request: *const FORMATETC) -> windows::core::HRESULT {
            let request = unsafe { &*request };
            if self.formats.iter().any(|(id, index, _, medium)| {
                *id == request.cfFormat && *index == request.lindex && request.tymed & medium != 0
            }) {
                windows::core::HRESULT(0)
            } else {
                E_NOTIMPL
            }
        }
        fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> windows::core::Result<()> {
            Err(E_NOTIMPL.into())
        }
        fn GetCanonicalFormatEtc(
            &self,
            _: *const FORMATETC,
            _: *mut FORMATETC,
        ) -> windows::core::HRESULT {
            E_NOTIMPL
        }
        fn SetData(
            &self,
            _: *const FORMATETC,
            _: *const STGMEDIUM,
            _: BOOL,
        ) -> windows::core::Result<()> {
            Err(E_NOTIMPL.into())
        }
        fn EnumFormatEtc(&self, _: u32) -> windows::core::Result<IEnumFORMATETC> {
            Err(E_NOTIMPL.into())
        }
        fn DAdvise(
            &self,
            _: *const FORMATETC,
            _: u32,
            _: Ref<IAdviseSink>,
        ) -> windows::core::Result<u32> {
            Err(E_NOTIMPL.into())
        }
        fn DUnadvise(&self, _: u32) -> windows::core::Result<()> {
            Err(E_NOTIMPL.into())
        }
        fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
            Err(E_NOTIMPL.into())
        }
    }

    fn file_descriptor(name: &str) -> Vec<u8> {
        let name: Vec<_> = name.encode_utf16().collect();
        let mut filename = [0u16; 260];
        filename[..name.len()].copy_from_slice(&name);
        let descriptor = FILEDESCRIPTORW {
            cFileName: filename,
            ..Default::default()
        };
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend_from_slice(unsafe {
            std::slice::from_raw_parts(
                (&descriptor as *const FILEDESCRIPTORW).cast::<u8>(),
                std::mem::size_of::<FILEDESCRIPTORW>(),
            )
        });
        data
    }

    #[test]
    fn virtual_file_receive_negotiates_stream_only_contents() {
        let original: Vec<u8> = (0..150_000).map(|n| (n % 251) as u8).collect();
        let data: IDataObject = Sample {
            formats: vec![
                (
                    registered("FileGroupDescriptorW"),
                    -1,
                    file_descriptor("收到.gif"),
                    TYMED_HGLOBAL.0 as u32,
                ),
                (
                    registered("FileContents"),
                    0,
                    original.clone(),
                    TYMED_ISTREAM.0 as u32,
                ),
            ],
            ..Default::default()
        }
        .into();
        let wrong = format(registered("FileContents"), 0, TYMED_HGLOBAL.0 as u32);
        assert!(unsafe { data.QueryGetData(&wrong) }.is_err());
        assert!(get(&data, &wrong).is_err());
        let received = receive(&data).unwrap();
        assert_eq!(std::fs::read(&received.paths[0]).unwrap(), original);
        let path = received.paths[0].clone();
        drop(received);
        assert!(!path.exists());
    }

    #[test]
    fn slow_async_source_returns_before_data_is_available() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        };
        let _apartment = ComApartment::enter().unwrap();
        let state = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(AtomicBool::new(false));
        let data: IDataObject = Sample {
            formats: vec![
                (
                    registered("FileGroupDescriptorW"),
                    -1,
                    file_descriptor("收到.gif"),
                    TYMED_HGLOBAL.0 as u32,
                ),
                (
                    registered("FileContents"),
                    0,
                    b"GIF89a".to_vec(),
                    TYMED_ISTREAM.0 as u32,
                ),
            ],
            async_state: Some(state.clone()),
            release_data: Some(release.clone()),
        }
        .into();
        let (send, receive) = std::sync::mpsc::channel();
        receive_in_background(&data, move |result| {
            send.send(result).unwrap();
        })
        .unwrap();
        assert_eq!(state.load(Ordering::SeqCst), 1);
        assert!(receive.try_recv().is_err());
        drop(data); // 后台持有封送引用，原调用方的引用可以释放。
        release.store(true, Ordering::SeqCst);
        let received = receive
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(&received.paths[0]).unwrap(), b"GIF89a");
        assert_eq!(state.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn background_receive_reports_failures_and_finishes_async_operation() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let _apartment = ComApartment::enter().unwrap();
        let state = Arc::new(AtomicUsize::new(0));
        let data: IDataObject = Sample {
            async_state: Some(state.clone()),
            ..Default::default()
        }
        .into();
        let (send, receive) = std::sync::mpsc::channel();
        receive_in_background(&data, move |result| {
            send.send(result.err()).unwrap();
        })
        .unwrap();
        assert!(
            receive
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .unwrap()
                .contains("没有提供")
        );
        assert_eq!(state.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn synchronous_source_is_read_on_worker_before_drop_returns() {
        let _apartment = ComApartment::enter().unwrap();
        let data: IDataObject = Sample {
            formats: vec![(
                registered("PNG"),
                -1,
                b"png bytes".to_vec(),
                TYMED_HGLOBAL.0 as u32,
            )],
            ..Default::default()
        }
        .into();
        let caller = std::thread::current().id();
        let (send, receive) = std::sync::mpsc::channel();
        receive_in_background(&data, move |result| {
            send.send((std::thread::current().id(), result)).unwrap();
        })
        .unwrap();
        let (worker, received) = receive.try_recv().unwrap();
        assert_ne!(caller, worker);
        assert_eq!(
            std::fs::read(&received.unwrap().paths[0]).unwrap(),
            b"png bytes"
        );
    }

    #[test]
    fn file_paths_keep_the_source_file_and_streams_keep_all_chunks() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("收到.png");
        std::fs::write(&path, b"original").unwrap();
        let mut hdrop = 20u32.to_le_bytes().to_vec();
        hdrop.extend_from_slice(&[0; 12]);
        hdrop.extend_from_slice(&1u32.to_le_bytes());
        for value in path.as_os_str().encode_wide().chain([0, 0]) {
            hdrop.extend_from_slice(&value.to_le_bytes());
        }
        let data: IDataObject = Sample {
            formats: vec![(15, -1, hdrop, TYMED_HGLOBAL.0 as u32)],
            ..Default::default()
        }
        .into();
        let received = receive(&data).unwrap();
        assert_eq!(received.paths.as_slice(), std::slice::from_ref(&path));
        drop(received);
        assert_eq!(std::fs::read(path).unwrap(), b"original");

        let content: Vec<u8> = (0..150_000).map(|n| (n % 251) as u8).collect();
        let stream =
            unsafe { windows::Win32::UI::Shell::SHCreateMemStream(Some(&content)) }.unwrap();
        let mut medium = Medium(STGMEDIUM {
            tymed: TYMED_ISTREAM.0 as u32,
            ..Default::default()
        });
        medium.0.u.pstm = std::mem::ManuallyDrop::new(Some(stream));
        assert_eq!(bytes(&medium).unwrap(), content);
    }

    #[test]
    fn virtual_file_data_is_staged_without_changing_original_bytes() {
        let original = b"GIF89a original animation bytes".to_vec();
        let data: IDataObject = Sample {
            formats: vec![
                (
                    registered("FileGroupDescriptorW"),
                    -1,
                    file_descriptor("收到.gif"),
                    TYMED_HGLOBAL.0 as u32,
                ),
                (
                    registered("FileContents"),
                    0,
                    original.clone(),
                    TYMED_HGLOBAL.0 as u32,
                ),
            ],
            ..Default::default()
        }
        .into();
        let received = receive(&data).unwrap();
        assert_eq!(received.paths.len(), 1);
        let path = received.paths[0].clone();
        assert_eq!(path.file_name().unwrap(), "收到.gif");
        assert_eq!(std::fs::read(&path).unwrap(), original);
        drop(received);
        assert!(!path.exists());
    }

    #[test]
    fn png_data_is_staged_and_unsupported_sources_report_an_error() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([23, 45, 67, 128]),
        ));
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let original = png.into_inner();
        let data: IDataObject = Sample {
            formats: vec![(
                registered("PNG"),
                -1,
                original.clone(),
                TYMED_HGLOBAL.0 as u32,
            )],
            ..Default::default()
        }
        .into();
        let received = receive(&data).unwrap();
        assert_eq!(std::fs::read(&received.paths[0]).unwrap(), original);
        let unsupported: IDataObject = Sample::default().into();
        assert!(receive(&unsupported).err().unwrap().contains("没有提供"));
    }

    #[test]
    fn virtual_names_cannot_escape_the_staging_directory() {
        let mut data = file_descriptor("../../收到.gif");
        assert_eq!(descriptors(&data).unwrap(), ["收到.gif"]);
        data.truncate(data.len() - 1);
        assert!(descriptors(&data).is_err());
        assert!(descriptors(&u32::MAX.to_le_bytes()).is_err());
    }

    #[test]
    fn dib_round_trip_keeps_pixels_and_rejects_invalid_headers() {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            2,
            3,
            image::Rgb([23, 45, 67]),
        ));
        let mut bmp = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bmp, image::ImageFormat::Bmp).unwrap();
        let png = dib_png(&bmp.into_inner()[14..]).unwrap();
        let decoded = image::load_from_memory(&png).unwrap().to_rgb8();
        assert_eq!(decoded, image.to_rgb8());
        assert!(dib_png(&[0; 39]).is_err());
        let mut malformed = vec![0; 40];
        malformed[..4].copy_from_slice(&40u32.to_le_bytes());
        malformed[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(dib_png(&malformed).is_err());
    }
}
