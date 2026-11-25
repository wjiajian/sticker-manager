//! Windows 原生文件拖出：在独立消息窗口中运行模态 `DoDragDrop`，
//! 避免阻塞 `tao` 事件回调导致窗口内 `tauri://drag-*` 事件全部滞留；
//! 同时使用 32 位预乘 DIB 构造 `SHDRAGIMAGE`，保证拖拽时跟随鼠标显示清晰的表情预览。
use std::{
    cell::Cell,
    ffi::OsStr,
    iter::once,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
    sync::mpsc,
};

use image::RgbaImage;
use windows::{
    Win32::{
        Foundation::{
            COLORREF, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, HINSTANCE,
            HWND, LPARAM, LRESULT, POINT, S_OK, SIZE, WPARAM,
        },
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, DeleteObject,
            HBITMAP, HGDIOBJ,
        },
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IDataObject},
            LibraryLoader::GetModuleHandleW,
            Ole::{
                DROPEFFECT, DROPEFFECT_COPY, DoDragDrop, IDropSource, IDropSource_Impl,
                OleInitialize,
            },
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
        },
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON},
            Shell::{
                BHID_DataObject, CLSID_DragDropHelper, Common::ITEMIDLIST, IDragSourceHelper,
                ILCreateFromPathW, ILFree, IShellItemArray, SHCreateShellItemArrayFromIDLists,
                SHDRAGIMAGE,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, HWND_MESSAGE,
                PostMessageW, RegisterClassW, SM_SWAPBUTTON, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP,
                WNDCLASSW,
            },
        },
    },
    core::{BOOL, HRESULT, PCWSTR, w},
};

const WM_START_DRAG: u32 = WM_APP + 1;
const MAX_DRAG_IMAGE_SIDE: u32 = 300;
/// `CLR_NONE`：告知 Shell 拖拽图像使用 32 位像素自带的 Alpha 通道，不把纯黑等像素抠成透明。
const CLR_NONE: COLORREF = COLORREF(0xFFFF_FFFF);

thread_local! {
    static DRAG_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

struct DragRequest {
    path: PathBuf,
    icon_path: PathBuf,
    reply: mpsc::Sender<Result<bool, String>>,
}

/// 主线程隐藏消息窗口，用于把 `DoDragDrop` 调度到 `tao` 事件回调之外执行。
pub struct DragHost {
    hwnd: isize,
}

impl DragHost {
    pub fn new() -> Result<Self, String> {
        let class_name = w!("StickerManagerDragHost");
        // SAFETY: 在应用主线程创建仅供本模块使用的消息窗口；`wnd_proc` 只处理 `WM_START_DRAG`。
        unsafe {
            let _ = OleInitialize(None);
            let module = GetModuleHandleW(PCWSTR::null()).map_err(|e| e.to_string())?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: HINSTANCE(module.0),
                lpszClassName: class_name,
                ..Default::default()
            };
            let _ = RegisterClassW(&class);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                PCWSTR::null(),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(HINSTANCE(module.0)),
                None,
            )
            .map_err(|e| e.to_string())?;
            Ok(Self {
                hwnd: hwnd.0 as isize,
            })
        }
    }

    pub async fn start_drag(&self, path: PathBuf, icon_path: PathBuf) -> Result<bool, String> {
        let (tx, rx) = mpsc::channel();
        let boxed = Box::into_raw(Box::new(DragRequest {
            path,
            icon_path,
            reply: tx,
        }));
        // SAFETY: `PostMessageW` 线程安全；失败时立刻回收 `boxed`，成功时由 `wnd_proc` 回收。
        let posted = unsafe {
            PostMessageW(
                Some(HWND(self.hwnd as _)),
                WM_START_DRAG,
                WPARAM(0),
                LPARAM(boxed as isize),
            )
        };
        if let Err(error) = posted {
            unsafe { drop(Box::from_raw(boxed)) };
            return Err(format!("无法启动系统拖拽：{error}"));
        }
        tauri::async_runtime::spawn_blocking(move || {
            rx.recv().map_err(|_| "系统拖拽意外中断".to_string())?
        })
        .await
        .map_err(|error| error.to_string())?
    }
}

impl Drop for DragHost {
    fn drop(&mut self) {
        if self.hwnd != 0 {
            // SAFETY: 窗口由 `DragHost::new` 创建，退出时销毁。
            let _ = unsafe { DestroyWindow(HWND(self.hwnd as _)) };
        }
    }
}

struct ActiveGuard;
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        DRAG_ACTIVE.with(|active| active.set(false));
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_START_DRAG {
        // SAFETY: `lparam` 是 `start_drag` 用 `Box::into_raw` 构造的 `DragRequest`。
        let request = unsafe { Box::from_raw(lparam.0 as *mut DragRequest) };
        let result = if DRAG_ACTIVE.with(|active| active.replace(true)) {
            Ok(false)
        } else {
            let _guard = ActiveGuard;
            if !primary_button_pressed() {
                Ok(false)
            } else {
                do_drag(&request.path, &request.icon_path)
            }
        };
        let _ = request.reply.send(result);
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn primary_button_pressed() -> bool {
    // SAFETY: Win32 只读系统状态查询。
    unsafe {
        let vk = if GetSystemMetrics(SM_SWAPBUTTON) != 0 {
            VK_RBUTTON
        } else {
            VK_LBUTTON
        };
        (GetAsyncKeyState(i32::from(vk.0)) as u16 & 0x8000) != 0
    }
}

#[windows::core::implement(IDropSource)]
struct DropSource;

#[allow(non_snake_case)]
impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, escape_pressed: BOOL, key_state: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape_pressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if (key_state & MK_LBUTTON) == MODIFIERKEYS_FLAGS(0) {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

struct Pidl(*mut ITEMIDLIST);
impl Drop for Pidl {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: 由 `ILCreateFromPathW` 分配的 PIDL。
            unsafe { ILFree(Some(self.0.cast_const())) };
        }
    }
}

struct OwnedBitmap(HBITMAP);
impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: 未移交给 `IDragSourceHelper` 时释放 GDI 位图句柄。
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.0.0));
            }
        }
    }
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(once(0)).collect()
}

/// 去掉 `std::fs::canonicalize` 在 Windows 上附加的 `\\?\` 扩展路径前缀，
/// 避免 `ILCreateFromPathW` 无法解析。
fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

fn normalize_shell_path(path: &Path) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(path).map_err(|e| format!("无法访问待拖出文件：{e}"))?;
    Ok(strip_verbatim_prefix(&canonical))
}

fn shell_data_object(path: &Path) -> Result<IDataObject, String> {
    let wide = wide_null(path.as_os_str());
    // SAFETY: `wide` 以 NUL 结尾，`Pidl` 负责释放，`array` 在 `BindToHandler` 期间存活。
    unsafe {
        let pidl = Pidl(ILCreateFromPathW(PCWSTR::from_raw(wide.as_ptr())));
        if pidl.0.is_null() {
            return Err("无法创建拖拽文件对象".into());
        }
        let list = [pidl.0.cast_const()];
        let array: IShellItemArray =
            SHCreateShellItemArrayFromIDLists(&list).map_err(|e| e.to_string())?;
        array
            .BindToHandler(None, &BHID_DataObject)
            .map_err(|e| e.to_string())
    }
}

/// 把 RGBA 图像写入自底向上的 32 位预乘 BGRA 像素缓冲（Windows `SHDRAGIMAGE` 格式）。
fn encode_premultiplied_bgra_bottom_up(image: &RgbaImage) -> Vec<u8> {
    let width = image.width() as usize;
    let height = image.height() as usize;
    let mut bgra = vec![0u8; width * height * 4];
    for y in 0..height {
        let dst_y = height - 1 - y;
        let dst_row = &mut bgra[dst_y * width * 4..(dst_y + 1) * width * 4];
        for x in 0..width {
            let [r, g, b, a] = image.get_pixel(x as u32, y as u32).0;
            let alpha = u16::from(a);
            let offset = x * 4;
            dst_row[offset] = ((u16::from(b) * alpha + 127) / 255) as u8;
            dst_row[offset + 1] = ((u16::from(g) * alpha + 127) / 255) as u8;
            dst_row[offset + 2] = ((u16::from(r) * alpha + 127) / 255) as u8;
            dst_row[offset + 3] = a;
        }
    }
    bgra
}

fn load_drag_image(icon_path: &Path) -> Result<(OwnedBitmap, i32, i32), String> {
    let decoded = image::open(icon_path).map_err(|e| e.to_string())?;
    let rgba = if decoded.width().max(decoded.height()) > MAX_DRAG_IMAGE_SIDE {
        decoded
            .thumbnail(MAX_DRAG_IMAGE_SIDE, MAX_DRAG_IMAGE_SIDE)
            .to_rgba8()
    } else {
        decoded.to_rgba8()
    };
    let width = rgba.width();
    let height = rgba.height();
    if width == 0 || height == 0 {
        return Err("拖拽预览图尺寸为空".into());
    }
    let pixels = encode_premultiplied_bgra_bottom_up(&rgba);
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: height as i32,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = ptr::null_mut();
    // SAFETY: `CreateDIBSection` 分配 `width * height * 4` 字节像素区，随后立刻复制完整数据。
    let bitmap = unsafe {
        CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|e| e.to_string())?
    };
    if bits.is_null() {
        let _ = OwnedBitmap(bitmap);
        return Err("无法分配拖拽预览位图".into());
    }
    unsafe {
        ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
    }
    Ok((OwnedBitmap(bitmap), width as i32, height as i32))
}

fn attach_drag_image(data_object: &IDataObject, icon_path: &Path) -> Result<(), String> {
    let (bitmap, width, height) = load_drag_image(icon_path)?;
    let drag_image = SHDRAGIMAGE {
        sizeDragImage: SIZE {
            cx: width,
            cy: height,
        },
        // `drag_icon` 在 Windows 下已在左上角留白，偏移设为 (0, 0) 可让预览浮在光标右下方。
        ptOffset: POINT { x: 0, y: 0 },
        hbmpDragImage: bitmap.0,
        crColorKey: CLR_NONE,
    };
    // SAFETY: `InitializeFromBitmap` 成功时会接管 `hbmpDragImage` 所有权；失败时由 `OwnedBitmap` 释放。
    unsafe {
        let helper: IDragSourceHelper =
            CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
        helper
            .InitializeFromBitmap(&drag_image, data_object)
            .map_err(|e| e.to_string())?;
    }
    std::mem::forget(bitmap);
    Ok(())
}

fn do_drag(path: &Path, icon_path: &Path) -> Result<bool, String> {
    let normalized = normalize_shell_path(path)?;
    let data_object = shell_data_object(&normalized)?;
    if let Err(error) = attach_drag_image(&data_object, icon_path) {
        eprintln!("附加 Windows 拖拽预览图失败：{error}");
    }
    let drop_source: IDropSource = DropSource.into();
    let mut out_effect = DROPEFFECT::default();
    // SAFETY: 已在主线程初始化 OLE，`data_object` 与 `drop_source` 在整个模态拖拽期间存活。
    let hr = unsafe { DoDragDrop(&data_object, &drop_source, DROPEFFECT_COPY, &mut out_effect) };
    if hr == DRAGDROP_S_DROP {
        Ok(true)
    } else if hr == DRAGDROP_S_CANCEL {
        Ok(false)
    } else {
        Err(format!("系统拖拽失败：{hr:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use windows::Win32::System::{
        Com::{DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL},
        DataExchange::RegisterClipboardFormatW,
        Ole::OleUninitialize,
    };
    use windows::core::HSTRING;

    #[test]
    fn strips_windows_verbatim_prefixes() {
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\C:\Users\test\a.gif")),
            PathBuf::from(r"C:\Users\test\a.gif")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\UNC\server\share\a.gif")),
            PathBuf::from(r"\\server\share\a.gif")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"C:\Users\test\a.gif")),
            PathBuf::from(r"C:\Users\test\a.gif")
        );
    }

    #[test]
    fn encodes_premultiplied_bottom_up_bgra() {
        let mut img = RgbaImage::new(2, 2);
        // 顶行左：半透明纯红；顶行右：纯黑不透明
        img.put_pixel(0, 0, Rgba([255, 0, 0, 128]));
        img.put_pixel(1, 0, Rgba([0, 0, 0, 255]));
        // 底行左：完全透明；底行右：纯白不透明
        img.put_pixel(0, 1, Rgba([100, 150, 200, 0]));
        img.put_pixel(1, 1, Rgba([255, 255, 255, 255]));

        let bgra = encode_premultiplied_bgra_bottom_up(&img);
        // DIB 第 0 行对应图像底行
        assert_eq!(&bgra[0..4], &[0, 0, 0, 0]);
        assert_eq!(&bgra[4..8], &[255, 255, 255, 255]);
        // DIB 第 1 行对应图像顶行
        assert_eq!(&bgra[8..12], &[0, 0, 128, 128]);
        assert_eq!(&bgra[12..16], &[0, 0, 0, 255]);
    }

    #[test]
    fn shell_data_object_accepts_drag_image_bits() {
        use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
        use windows::Win32::System::Ole::ReleaseStgMedium;

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cat.png");
        let icon = dir.path().join("icon.png");
        let mut img = RgbaImage::from_pixel(76, 76, Rgba([0, 0, 0, 0]));
        for y in 12..76 {
            for x in 12..76 {
                let pixel = match (x < 34, y < 44) {
                    (true, true) => Rgba([0, 0, 0, 255]),
                    (false, true) => Rgba([255, 255, 255, 255]),
                    (true, false) => Rgba([220, 40, 40, 255]),
                    (false, false) => Rgba([40, 120, 220, 255]),
                };
                img.put_pixel(x, y, pixel);
            }
        }
        img.save(&file).unwrap();
        img.save(&icon).unwrap();

        unsafe {
            let init = OleInitialize(None);
            let normalized = normalize_shell_path(&file).unwrap();
            let data = shell_data_object(&normalized).unwrap();
            attach_drag_image(&data, &icon).unwrap();

            let drag_image_bits = RegisterClipboardFormatW(&HSTRING::from("DragImageBits")) as u16;
            let req = FORMATETC {
                cfFormat: drag_image_bits,
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
                ..Default::default()
            };
            assert_eq!(data.QueryGetData(&req), S_OK);
            let mut medium = data.GetData(&req).unwrap();
            let hglobal = medium.u.hGlobal;
            let total_size = GlobalSize(hglobal);
            let pixel_bytes = 76 * 76 * 4;
            assert!(
                total_size >= pixel_bytes + 24,
                "DragImageBits size={total_size} unexpected"
            );
            let raw = GlobalLock(hglobal) as *const u8;
            let header = std::slice::from_raw_parts(raw as *const i32, 6);
            assert_eq!((header[0], header[1]), (76, 76));
            let bits = std::slice::from_raw_parts(raw.add(24), pixel_bytes);
            let alpha_at = |x: usize, y_top: usize| -> u8 {
                let dib_y = 75 - y_top;
                bits[(dib_y * 76 + x) * 4 + 3]
            };
            let transparent_alpha = alpha_at(2, 2);
            let black_alpha = alpha_at(20, 20);
            let white_alpha = alpha_at(50, 20);
            let red_alpha = alpha_at(20, 55);
            let _ = GlobalUnlock(hglobal);
            ReleaseStgMedium(&mut medium);
            assert_eq!(transparent_alpha, 0, "透明留白应保持透明");
            assert!(black_alpha > 0, "纯黑像素不应被 colorKey 抠空");
            assert!(white_alpha > 0, "纯白像素不应被 colorKey 抠空");
            assert!(red_alpha > 0, "彩色像素应可见");
            if init.is_ok() {
                OleUninitialize();
            }
        }
    }
}
