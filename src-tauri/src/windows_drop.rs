//! Windows 聊天软件可能交出虚拟文件或图片数据，而不是磁盘路径。
use crate::{
    clipboard::{ClipboardRead, html},
    staging::{
        self, ExternalDrop, MAX_BYTES, MAX_ITEMS, Origin, Staged, Staging, StagingRoot,
        UNSUPPORTED_DROP,
    },
};
use serde::Serialize;
use std::path::Path;
use std::{cell::Cell, ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
use tauri::{Emitter, EventTarget, Manager};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HANDLE, HWND, LPARAM, POINT, POINTL, S_OK},
        Graphics::Gdi::ScreenToClient,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, COWAIT_DISPATCH_CALLS,
                COWAIT_DISPATCH_WINDOW_MESSAGES, CoCreateInstance, CoInitializeEx, CoUninitialize,
                CoWaitForMultipleHandles, DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM,
                TYMED_HGLOBAL, TYMED_ISTREAM,
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
            Shell::{
                CLSID_DragDropHelper, DragQueryFileW, FILEDESCRIPTORW, HDROP,
                IDataObjectAsyncCapability, IDropTargetHelper,
            },
            WindowsAndMessaging::EnumChildWindows,
        },
    },
    core::{AgileReference, BOOL, HSTRING, Interface, Ref, implement},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Drop,
    Clipboard,
}
fn animated_formats() -> [u16; 3] {
    [
        registered("GIF"),
        registered("image/gif"),
        registered("image/webp"),
    ]
}
fn static_formats() -> [u16; 6] {
    [
        registered("PNG"),
        registered("image/png"),
        registered("JFIF"),
        registered("image/jpeg"),
        17,
        8,
    ]
}
fn supported_formats() -> impl Iterator<Item = u16> {
    [15, registered("FileGroupDescriptorW")]
        .into_iter()
        .chain(animated_formats())
        .chain(static_formats())
}

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

fn supports_drop(data: &IDataObject) -> bool {
    supported_formats()
        // 微信内部表情拖拽可能返回 S_FALSE；HRESULT::is_ok() 会把它也当成成功。
        // QueryGetData 只有 S_OK 才表示请求的格式可提供。
        .any(|id| unsafe { data.QueryGetData(&format(id, -1, TYMED_HGLOBAL.0 as u32)) == S_OK })
}

fn copy_effect(supported: bool, allowed: DROPEFFECT) -> DROPEFFECT {
    if supported && allowed.0 & DROPEFFECT_COPY.0 != 0 {
        DROPEFFECT_COPY
    } else {
        DROPEFFECT_NONE
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
    let result = unsafe { data.GetData(request) };
    crate::diagnostics::drop_diag!(
        "drop GetData format={} index={} medium={} result={:?}",
        request.cfFormat,
        request.lindex,
        request.tymed,
        result
            .as_ref()
            .map(|medium| medium.tymed)
            .map_err(|e| e.code())
    );
    result.map(Medium).map_err(|e| e.to_string())
}

pub fn diagnose_formats(data: &IDataObject, stage: &str) {
    #[cfg(debug_assertions)]
    if crate::diagnostics::enabled() {
        use windows::Win32::System::{
            Com::{CoTaskMemFree, DATADIR_GET},
            DataExchange::GetClipboardFormatNameW,
        };
        // 只记录格式和结果码，不读取文件名、路径或图片内容。
        unsafe {
            for id in supported_formats().chain([registered("HTML Format")]) {
                let request = format(id, -1, TYMED_HGLOBAL.0 as u32);
                eprintln!(
                    "drop {stage} probe={id} query={:?}",
                    data.QueryGetData(&request)
                );
            }
            eprintln!(
                "drop {stage} async={:?}",
                data.cast::<IDataObjectAsyncCapability>()
                    .and_then(|op| op.GetAsyncMode())
            );
            let formats = match data.EnumFormatEtc(DATADIR_GET.0 as u32) {
                Ok(formats) => formats,
                Err(error) => {
                    eprintln!("drop {stage} EnumFormatEtc={:?}", error.code());
                    return;
                }
            };
            for _ in 0..64 {
                let mut entry = [FORMATETC::default()];
                let mut fetched = 0;
                let result = formats.Next(&mut entry, Some(&mut fetched));
                if result.is_err() || fetched == 0 {
                    eprintln!("drop {stage} enumeration result={result:?} fetched={fetched}");
                    break;
                }
                let request = &entry[0];
                let mut name = [0u16; 256];
                let length =
                    GetClipboardFormatNameW(request.cfFormat as u32, &mut name).max(0) as usize;
                eprintln!(
                    "drop {stage} format={} name={} index={} medium={} aspect={} query={:?}",
                    request.cfFormat,
                    String::from_utf16_lossy(&name[..length]),
                    request.lindex,
                    request.tymed,
                    request.dwAspect,
                    data.QueryGetData(request)
                );
                if !request.ptd.is_null() {
                    CoTaskMemFree(Some(request.ptd.cast()));
                }
            }
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = (data, stage);
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
    if count == 0 || count > MAX_ITEMS || bytes.len() < 4 + count * size {
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

pub(crate) fn dib_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
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
    let compression = read_u32(16);
    if bits == 32 && matches!(compression, 0 | 3 | 6) {
        let width = i32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let height = i32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let w = usize::try_from(width).map_err(|_| "位图宽度无效")?;
        let h = height.unsigned_abs() as usize;
        let size = w
            .checked_mul(h)
            .and_then(|n| n.checked_mul(4))
            .filter(|n| *n > 0 && *n <= MAX_BYTES)
            .ok_or("位图尺寸无效")?;
        let pixels = bytes
            .get(offset..offset.checked_add(size).ok_or("位图尺寸无效")?)
            .ok_or("位图像素数据不完整")?;
        let masks = if compression == 0 {
            [0xff0000, 0xff00, 0xff, 0]
        } else {
            let alpha = if header >= 108 || compression == 6 {
                read_u32(52) as u32
            } else {
                0
            };
            [
                read_u32(40) as u32,
                read_u32(44) as u32,
                read_u32(48) as u32,
                alpha,
            ]
        };
        if masks[..3].contains(&0)
            || masks
                .iter()
                .enumerate()
                .any(|(i, &mask)| masks[..i].iter().any(|other| mask & other != 0))
        {
            return Err("位图通道掩码无效".into());
        }
        let component = |pixel: u32, mask: u32| {
            if mask == 0 {
                return 255;
            }
            let shift = mask.trailing_zeros();
            let max = mask >> shift;
            ((((pixel & mask) >> shift) as u64 * 255 + u64::from(max) / 2) / u64::from(max)) as u8
        };
        let mut rgba = Vec::with_capacity(size);
        for row in 0..h {
            let y = if height < 0 { row } else { h - row - 1 };
            for pixel in pixels[y * w * 4..(y + 1) * w * 4].chunks_exact(4) {
                let pixel = u32::from_le_bytes(pixel.try_into().unwrap());
                rgba.extend(masks.map(|mask| component(pixel, mask)));
            }
        }
        let image = image::RgbaImage::from_raw(w as u32, h as u32, rgba).ok_or("位图尺寸无效")?;
        let mut png = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        return Ok(png.into_inner());
    }
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
    root: PathBuf,
    complete: impl FnOnce(Result<Staged, String>) + Send + 'static,
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
                let data_reference = reference;
                let completion_reference = operation_reference;
                let operation = completion_reference
                    .as_ref()
                    .map(AgileReference::resolve)
                    .transpose()
                    .map_err(|e| e.to_string())?;
                let result = data_reference
                    .resolve()
                    .map_err(|e| e.to_string())
                    .and_then(|data| receive_drop(&data, &root));
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
                drop(data_reference);
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

fn read_hdrop(data: &IDataObject, limit: Option<usize>) -> Result<Vec<PathBuf>, String> {
    let request = format(15, -1, TYMED_HGLOBAL.0 as u32);
    if unsafe { data.QueryGetData(&request) } != S_OK {
        return Err(UNSUPPORTED_DROP.into());
    }
    let medium = get(data, &request)?;
    if medium.0.tymed != TYMED_HGLOBAL.0 as u32 {
        return Err("拖入文件介质无效".into());
    }
    let drop = unsafe { HDROP(medium.0.u.hGlobal.0) };
    let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
    if count == 0 || limit.is_some_and(|max| count as usize > max) {
        return Err("拖入文件为空或超过 256 项".into());
    }
    let mut paths = Vec::with_capacity(count as usize);
    for index in 0..count {
        let length = unsafe { DragQueryFileW(drop, index, None) };
        let mut name = vec![0u16; length as usize + 1];
        unsafe {
            DragQueryFileW(drop, index, Some(&mut name));
        }
        paths.push(PathBuf::from(OsString::from_wide(&name[..length as usize])));
    }
    Ok(paths)
}

pub fn receive(data: &IDataObject, source: Source, root: &Path) -> Result<ClipboardRead, String> {
    diagnose_formats(data, "receive");
    if unsafe { data.QueryGetData(&format(15, -1, TYMED_HGLOBAL.0 as u32)) == S_OK } {
        return Ok(ClipboardRead::Staged(Staged::existing(read_hdrop(
            data,
            Some(MAX_ITEMS),
        )?)));
    }
    let mut staged = Staging::new(
        root,
        if source == Source::Drop {
            Origin::Drop
        } else {
            Origin::Clipboard
        },
    )?;
    if let Ok(medium) = get(
        data,
        &format(
            registered("FileGroupDescriptorW"),
            -1,
            TYMED_HGLOBAL.0 as u32,
        ),
    ) {
        let names = descriptors(&bytes(&medium)?)?;
        for (index, name) in names.iter().enumerate() {
            let result = get(
                data,
                &format(
                    registered("FileContents"),
                    index as i32,
                    (TYMED_HGLOBAL.0 | TYMED_ISTREAM.0) as u32,
                ),
            )
            .and_then(|medium| bytes(&medium))
            .and_then(|content| staged.add_named(name, &content));
            if let Err(error) = result {
                staged.fail(name, error, 1);
            }
        }
        return Ok(ClipboardRead::Staged(staged.finish()));
    }
    for id in animated_formats() {
        if let Ok(medium) = get(data, &format(id, -1, TYMED_HGLOBAL.0 as u32))
            && let Ok(content) = bytes(&medium)
            && staged.add_bytes(&content).is_ok()
        {
            return Ok(ClipboardRead::Staged(staged.finish()));
        }
    }
    let mut remote_only = false;
    if source == Source::Clipboard
        && let Ok(medium) = get(
            data,
            &format(registered("HTML Format"), -1, TYMED_HGLOBAL.0 as u32),
        )
        && let Ok(content) = bytes(&medium)
    {
        let images = html::images(&String::from_utf8_lossy(&content), &mut staged);
        remote_only = images.remote_only;
        if !images.paths.is_empty() {
            let mut staged = staged.finish();
            staged.paths = images.paths;
            return Ok(ClipboardRead::Staged(staged));
        }
    }
    for id in static_formats() {
        if let Ok(medium) = get(data, &format(id, -1, TYMED_HGLOBAL.0 as u32))
            && let Ok(content) = bytes(&medium)
        {
            let content = if id == 17 || id == 8 {
                dib_png(&content)
            } else {
                Ok(content)
            };
            if let Ok(content) = content
                && staged.add_bytes(&content).is_ok()
            {
                return Ok(ClipboardRead::Staged(staged.finish()));
            }
        }
    }
    Ok(ClipboardRead::Empty { remote_only })
}
fn receive_drop(data: &IDataObject, root: &Path) -> Result<Staged, String> {
    match receive(data, Source::Drop, root)? {
        ClipboardRead::Staged(staged) if staged.paths.is_empty() => {
            Err(staged.failures.first().map_or_else(
                || UNSUPPORTED_DROP.into(),
                |failure| failure.message.clone(),
            ))
        }
        ClipboardRead::Staged(staged) => Ok(staged),
        ClipboardRead::Empty { .. } => Err(UNSUPPORTED_DROP.into()),
    }
}

fn create_drop_target_helper() -> Option<IDropTargetHelper> {
    // SAFETY: 在 UI 线程 STA 中创建系统 Shell 拖拽图像辅助对象；失败时退化为不绘制拖拽图。
    unsafe { CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER).ok() }
}

fn screen_point(pt: &POINTL) -> POINT {
    POINT { x: pt.x, y: pt.y }
}

fn client_position(root: HWND, pt: &POINTL) -> DragDropPosition {
    let mut point = screen_point(pt);
    // SAFETY: `root` 为本应用主/面板窗口句柄，返回相对于窗口客户区左上角的物理像素坐标。
    let _ = unsafe { ScreenToClient(root, &mut point) };
    DragDropPosition {
        x: f64::from(point.x),
        y: f64::from(point.y),
    }
}

#[derive(Clone, Serialize)]
struct DragDropPosition {
    x: f64,
    y: f64,
}

#[derive(Clone, Serialize)]
struct DragDropPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    paths: Option<Vec<PathBuf>>,
    position: DragDropPosition,
}

#[implement(IDropTarget)]
struct Target {
    app: tauri::AppHandle,
    root: HWND,
    helper: Option<IDropTargetHelper>,
    accepted: Cell<bool>,
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let supported = data.as_ref().is_some_and(supports_drop);
        if let Some(data) = data.as_ref() {
            diagnose_formats(data, "DragEnter");
        }
        self.accepted.set(supported);
        let chosen;
        unsafe {
            chosen = copy_effect(supported, *effect);
            *effect = chosen;
        }
        if let Some(helper) = &self.helper {
            let point = screen_point(pt);
            let _ = unsafe { helper.DragEnter(self.root, data.as_ref(), &point, chosen) };
        }
        let accepted = chosen == DROPEFFECT_COPY;
        let _ = crate::ball::external_drop(&self.app, accepted);
        if !supported {
            let _ = crate::ball::ball_message(self.app.clone(), UNSUPPORTED_DROP.into());
        }
        Ok(())
    }
    fn DragOver(
        &self,
        _: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let chosen = unsafe {
            let chosen = copy_effect(self.accepted.get(), *effect);
            *effect = chosen;
            chosen
        };
        if let Some(helper) = &self.helper {
            let point = screen_point(pt);
            let _ = unsafe { helper.DragOver(&point, chosen) };
        }
        let _ = crate::ball::external_drop(&self.app, chosen == DROPEFFECT_COPY);
        Ok(())
    }
    fn DragLeave(&self) -> windows::core::Result<()> {
        self.accepted.set(false);
        if let Some(helper) = &self.helper {
            let _ = unsafe { helper.DragLeave() };
        }
        let _ = crate::ball::external_drop(&self.app, false);
        Ok(())
    }
    fn Drop(
        &self,
        data: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        if let Some(data) = data.as_ref() {
            diagnose_formats(data, "Drop");
        }
        let accepted = unsafe { copy_effect(data.as_ref().is_some_and(supports_drop), *effect) };
        self.accepted.set(false);
        let _ = crate::ball::external_drop(&self.app, false);
        unsafe {
            *effect = DROPEFFECT_NONE;
        }
        if accepted == DROPEFFECT_NONE {
            if let Some(helper) = &self.helper {
                let point = screen_point(pt);
                let _ = unsafe { helper.Drop(data.as_ref(), &point, DROPEFFECT_NONE) };
            }
            return Ok(());
        }
        let result = data
            .as_ref()
            .ok_or_else(|| "拖入数据为空".into())
            .and_then(|data| {
                let app = self.app.clone();
                let root = app.state::<StagingRoot>().0.clone();
                receive_in_background(data, root, move |received| {
                    tauri::async_runtime::spawn(staging::complete(app, received, true));
                })
            });
        let final_effect = match result {
            Ok(()) => DROPEFFECT_COPY,
            Err(error) => {
                let _ = crate::ball::ball_message(self.app.clone(), error);
                DROPEFFECT_NONE
            }
        };
        unsafe {
            *effect = final_effect;
        }
        if let Some(helper) = &self.helper {
            let point = screen_point(pt);
            let _ = unsafe { helper.Drop(data.as_ref(), &point, final_effect) };
        }
        Ok(())
    }
}

/// Preserve path events for category drops and panel; native data is imported on main only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    None,
    Paths,
    Native,
}
fn webview_mode(paths: bool, supported: bool, main: bool, copy: bool) -> Mode {
    match crate::drop_policy::classify(paths, supported && main, false, copy) {
        crate::drop_policy::DragDecision::Wry => Mode::Paths,
        crate::drop_policy::DragDecision::Native => Mode::Native,
        crate::drop_policy::DragDecision::Rejected(_) => Mode::None,
    }
}
#[implement(IDropTarget)]
struct WebviewDropTarget {
    app: tauri::AppHandle,
    label: String,
    root: HWND,
    helper: Option<IDropTargetHelper>,
    mode: Cell<Mode>,
}
impl WebviewDropTarget_Impl {
    fn leave(&self) {
        match self.mode.replace(Mode::None) {
            Mode::Paths => {
                let _ =
                    self.app
                        .emit_to(EventTarget::labeled(&self.label), "tauri://drag-leave", ());
            }
            Mode::Native => staging::emit(&self.app, ExternalDrop::Leave),
            Mode::None => {}
        }
    }
}
#[allow(non_snake_case)]
impl IDropTarget_Impl for WebviewDropTarget_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let paths = data
            .as_ref()
            .and_then(|data| read_hdrop(data, None).ok())
            .unwrap_or_default();
        let supported = data.as_ref().is_some_and(supports_drop);
        let mode = webview_mode(!paths.is_empty(), supported, self.label == "main", unsafe {
            (*effect).0 & DROPEFFECT_COPY.0 != 0
        });
        self.mode.set(mode);
        let chosen = unsafe { copy_effect(mode != Mode::None, *effect) };
        unsafe {
            *effect = chosen;
        }
        if let Some(helper) = &self.helper {
            let point = screen_point(pt);
            let _ = unsafe { helper.DragEnter(self.root, data.as_ref(), &point, chosen) };
        }
        match mode {
            Mode::Paths => {
                let _ = self.app.emit_to(
                    EventTarget::labeled(&self.label),
                    "tauri://drag-enter",
                    DragDropPayload {
                        paths: Some(paths),
                        position: client_position(self.root, pt),
                    },
                );
            }
            Mode::Native => staging::emit(&self.app, ExternalDrop::Enter),
            Mode::None if self.label == "main" && !supported && paths.is_empty() => staging::emit(
                &self.app,
                ExternalDrop::Rejected {
                    message: UNSUPPORTED_DROP.into(),
                },
            ),
            Mode::None => {}
        }
        Ok(())
    }
    fn DragOver(
        &self,
        _: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let chosen = unsafe { copy_effect(self.mode.get() != Mode::None, *effect) };
        unsafe {
            *effect = chosen;
        }
        if let Some(helper) = &self.helper {
            let point = screen_point(pt);
            let _ = unsafe { helper.DragOver(&point, chosen) };
        }
        if self.mode.get() == Mode::Paths && chosen != DROPEFFECT_NONE {
            let _ = self.app.emit_to(
                EventTarget::labeled(&self.label),
                "tauri://drag-over",
                DragDropPayload {
                    paths: None,
                    position: client_position(self.root, pt),
                },
            );
        } else if self.mode.get() == Mode::Native {
            staging::emit(
                &self.app,
                if chosen == DROPEFFECT_NONE {
                    ExternalDrop::Leave
                } else {
                    ExternalDrop::Enter
                },
            );
        }
        Ok(())
    }
    fn DragLeave(&self) -> windows::core::Result<()> {
        if let Some(helper) = &self.helper {
            let _ = unsafe { helper.DragLeave() };
        }
        self.leave();
        Ok(())
    }
    fn Drop(
        &self,
        data: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let mode = self.mode.get();
        let mut chosen = unsafe { copy_effect(mode != Mode::None, *effect) };
        if chosen == DROPEFFECT_NONE {
            self.leave();
        } else if mode == Mode::Paths {
            self.mode.set(Mode::None);
            let paths = data
                .as_ref()
                .and_then(|data| read_hdrop(data, None).ok())
                .unwrap_or_default();
            if paths.is_empty() {
                chosen = DROPEFFECT_NONE;
                let _ =
                    self.app
                        .emit_to(EventTarget::labeled(&self.label), "tauri://drag-leave", ());
            } else {
                let _ = self.app.emit_to(
                    EventTarget::labeled(&self.label),
                    "tauri://drag-drop",
                    DragDropPayload {
                        paths: Some(paths),
                        position: client_position(self.root, pt),
                    },
                );
            }
        } else if mode == Mode::Native {
            self.leave();
            staging::emit(&self.app, ExternalDrop::Receiving);
            let result = data
                .as_ref()
                .ok_or_else(|| "拖入数据为空".to_string())
                .and_then(|data| {
                    let app = self.app.clone();
                    let root = app.state::<StagingRoot>().0.clone();
                    receive_in_background(data, root, move |received| {
                        tauri::async_runtime::spawn(staging::complete(app, received, false));
                    })
                });
            if let Err(message) = result {
                chosen = DROPEFFECT_NONE;
                staging::emit(&self.app, ExternalDrop::Failed { message });
            }
        }
        unsafe {
            *effect = chosen;
        }
        if let Some(helper) = &self.helper {
            let point = screen_point(pt);
            let _ = unsafe { helper.Drop(data.as_ref(), &point, chosen) };
        }
        Ok(())
    }
}

fn register_on_window(window: &tauri::WebviewWindow, target: IDropTarget) -> Result<(), String> {
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

pub fn install(window: &tauri::WebviewWindow) -> Result<(), String> {
    let root = window.hwnd().map_err(|e| e.to_string())?;
    let target: IDropTarget = Target {
        app: window.app_handle().clone(),
        root,
        helper: create_drop_target_helper(),
        accepted: Cell::new(false),
    }
    .into();
    register_on_window(window, target)
}

pub fn install_webview(window: &tauri::WebviewWindow) -> Result<(), String> {
    let root = window.hwnd().map_err(|e| e.to_string())?;
    let target: IDropTarget = WebviewDropTarget {
        app: window.app_handle().clone(),
        label: window.label().to_string(),
        root,
        helper: create_drop_target_helper(),
        mode: Cell::new(Mode::None),
    }
    .into();
    register_on_window(window, target)
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
        query_result: Option<windows::core::HRESULT>,
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
            if request.cfFormat == registered("FileContents")
                && let Some(release) = &self.release_data
            {
                let start = std::time::Instant::now();
                while !release.load(std::sync::atomic::Ordering::SeqCst) {
                    if start.elapsed() > std::time::Duration::from_secs(3) {
                        return Err(E_FAIL.into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
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
            if let Some(result) = self.query_result {
                return result;
            }
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

    #[test]
    fn internal_wechat_drag_s_false_is_not_an_importable_format() {
        for status in [
            windows::Win32::Foundation::S_FALSE,
            windows::core::HRESULT(2),
        ] {
            let data: IDataObject = Sample {
                query_result: Some(status),
                ..Default::default()
            }
            .into();
            assert!(!supports_drop(&data));
            assert_eq!(
                copy_effect(supports_drop(&data), DROPEFFECT_COPY),
                DROPEFFECT_NONE
            );
        }
        let unsupported: IDataObject = Sample::default().into();
        assert!(!supports_drop(&unsupported));
    }

    #[test]
    fn supported_formats_require_an_explicit_copy_operation() {
        use windows::Win32::System::Ole::DROPEFFECT_MOVE;
        for id in supported_formats() {
            let data: IDataObject = Sample {
                formats: vec![(id, -1, vec![1], TYMED_HGLOBAL.0 as u32)],
                ..Default::default()
            }
            .into();
            assert!(supports_drop(&data));
            assert_eq!(copy_effect(true, DROPEFFECT_COPY), DROPEFFECT_COPY);
            assert_eq!(
                copy_effect(true, DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0)),
                DROPEFFECT_COPY
            );
            assert_eq!(copy_effect(true, DROPEFFECT_MOVE), DROPEFFECT_NONE);
            assert_eq!(copy_effect(true, DROPEFFECT_NONE), DROPEFFECT_NONE);
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
        let received = receive_drop(&data, &std::env::temp_dir()).unwrap();
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
            ..Default::default()
        }
        .into();
        let (send, receive) = std::sync::mpsc::channel();
        receive_in_background(&data, std::env::temp_dir(), move |result| {
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
        receive_in_background(&data, std::env::temp_dir(), move |result| {
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
                b"\x89PNG\r\n\x1a\n".to_vec(),
                TYMED_HGLOBAL.0 as u32,
            )],
            ..Default::default()
        }
        .into();
        let caller = std::thread::current().id();
        let (send, receive) = std::sync::mpsc::channel();
        receive_in_background(&data, std::env::temp_dir(), move |result| {
            send.send((std::thread::current().id(), result)).unwrap();
        })
        .unwrap();
        let (worker, received) = receive.try_recv().unwrap();
        assert_ne!(caller, worker);
        assert_eq!(
            std::fs::read(&received.unwrap().paths[0]).unwrap(),
            b"\x89PNG\r\n\x1a\n"
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
        let received = receive_drop(&data, &std::env::temp_dir()).unwrap();
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
        let received = receive_drop(&data, &std::env::temp_dir()).unwrap();
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
        let received = receive_drop(&data, &std::env::temp_dir()).unwrap();
        assert_eq!(std::fs::read(&received.paths[0]).unwrap(), original);
        let unsupported: IDataObject = Sample::default().into();
        assert!(
            receive_drop(&unsupported, &std::env::temp_dir())
                .err()
                .unwrap()
                .contains("没有提供")
        );
    }

    #[test]
    fn ole_images_import_into_library_and_outlive_staging() {
        for raw_png in [true, false] {
            let image = if raw_png {
                image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                    2,
                    3,
                    image::Rgba([23, 45, 67, 128]),
                ))
            } else {
                image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                    2,
                    3,
                    image::Rgb([23, 45, 67]),
                ))
            };
            let mut encoded = std::io::Cursor::new(Vec::new());
            let (id, content) = if raw_png {
                image
                    .write_to(&mut encoded, image::ImageFormat::Png)
                    .unwrap();
                (registered("PNG"), encoded.into_inner())
            } else {
                image
                    .write_to(&mut encoded, image::ImageFormat::Bmp)
                    .unwrap();
                (8, encoded.into_inner()[14..].to_vec())
            };
            let data: IDataObject = Sample {
                formats: vec![(id, -1, content, TYMED_HGLOBAL.0 as u32)],
                ..Default::default()
            }
            .into();
            let received = receive_drop(&data, &std::env::temp_dir()).unwrap();
            let staged = received.paths[0].clone();
            let root = tempfile::tempdir().unwrap();
            let mut library = sticker_core::Library::open(root.path()).unwrap();
            let report = library.import_paths(&received.paths);
            assert_eq!((report.added, report.skipped), (1, 0));
            assert!(report.failures.is_empty());
            let sticker = library.sticker(&report.sticker_ids[0]).unwrap();
            drop(received);
            assert!(!staged.exists());
            assert_eq!(
                image::open(sticker.object_path).unwrap().to_rgba8(),
                image.to_rgba8()
            );
            assert!(sticker.thumbnail_path.is_file());
        }
    }

    #[test]
    fn ole_virtual_animation_stream_imports_original_frames_and_deduplicates() {
        let mut original = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut original);
            for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
                encoder
                    .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                        2,
                        3,
                        image::Rgba(color),
                    )))
                    .unwrap();
            }
        }
        let data: IDataObject = Sample {
            formats: vec![
                (
                    registered("FileGroupDescriptorW"),
                    -1,
                    file_descriptor("聊天表情.gif"),
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
        let received = receive_drop(&data, &std::env::temp_dir()).unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut library = sticker_core::Library::open(root.path()).unwrap();
        let report = library.import_paths(&received.paths);
        assert_eq!((report.added, report.skipped), (1, 0));
        assert!(report.failures.is_empty());
        let repeated = library.import_paths(&received.paths);
        assert_eq!((repeated.added, repeated.existing), (0, 1));
        let sticker = library.sticker(&report.sticker_ids[0]).unwrap();
        drop(received);
        assert!(sticker.animated);
        assert_eq!((sticker.width, sticker.height), (2, 3));
        assert_eq!(std::fs::read(sticker.object_path).unwrap(), original);
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
    fn main_webview_accepts_native_and_prioritizes_paths() {
        assert_eq!(webview_mode(true, true, true, true), Mode::Paths);
        assert_eq!(webview_mode(false, true, true, true), Mode::Native);
        assert_eq!(webview_mode(false, true, false, true), Mode::None);
        assert_eq!(webview_mode(true, true, true, false), Mode::None);
    }
    #[test]
    fn dib32_top_down_and_v5_alpha() {
        for (header, compression, alpha) in [(40usize, 0u32, 255), (124, 3, 128)] {
            let mut dib = vec![0; header];
            dib[0..4].copy_from_slice(&(header as u32).to_le_bytes());
            dib[4..8].copy_from_slice(&1i32.to_le_bytes());
            dib[8..12].copy_from_slice(&(-2i32).to_le_bytes());
            dib[12..14].copy_from_slice(&1u16.to_le_bytes());
            dib[14..16].copy_from_slice(&32u16.to_le_bytes());
            dib[16..20].copy_from_slice(&compression.to_le_bytes());
            if compression == 3 {
                for (index, mask) in [0xff0000u32, 0xff00, 0xff, 0xff000000]
                    .into_iter()
                    .enumerate()
                {
                    dib[40 + index * 4..44 + index * 4].copy_from_slice(&mask.to_le_bytes());
                }
            }
            dib.extend_from_slice(&[67, 45, 23, 128, 12, 34, 56, 128]);
            let decoded = image::load_from_memory(&dib_png(&dib).unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(decoded.get_pixel(0, 0).0, [23, 45, 67, alpha]);
            assert_eq!(decoded.get_pixel(0, 1).0, [56, 34, 12, alpha]);
        }
    }
    #[test]
    fn clipboard_prefers_raw_animation_and_html_is_not_drag_format() {
        let png = b"\x89PNG\r\n\x1a\n".to_vec();
        let gif = b"GIF89a".to_vec();
        let data: IDataObject = Sample {
            formats: vec![
                (registered("PNG"), -1, png, TYMED_HGLOBAL.0 as u32),
                (
                    registered("image/gif"),
                    -1,
                    gif.clone(),
                    TYMED_HGLOBAL.0 as u32,
                ),
            ],
            ..Default::default()
        }
        .into();
        let root = tempfile::tempdir().unwrap();
        let ClipboardRead::Staged(staged) = receive(&data, Source::Clipboard, root.path()).unwrap()
        else {
            panic!("expected image")
        };
        assert_eq!(std::fs::read(&staged.paths[0]).unwrap(), gif);
        let data: IDataObject = Sample {
            formats: vec![(
                registered("HTML Format"),
                -1,
                b"<img src=data:image/gif;base64,R0lGODlh>".to_vec(),
                TYMED_HGLOBAL.0 as u32,
            )],
            ..Default::default()
        }
        .into();
        assert!(!supports_drop(&data));
        assert!(matches!(
            receive(&data, Source::Drop, root.path()).unwrap(),
            ClipboardRead::Empty { .. }
        ));
        assert!(matches!(
            receive(&data, Source::Clipboard, root.path()).unwrap(),
            ClipboardRead::Staged(_)
        ));
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
