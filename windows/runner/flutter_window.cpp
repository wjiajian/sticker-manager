#include "flutter_window.h"

#include <gdiplus.h>
#include <shellapi.h>
#include <shlobj.h>

#include <cstdint>
#include <cstring>
#include <iterator>
#include <limits>
#include <string>
#include <vector>

#include <optional>

#include "flutter/generated_plugin_registrant.h"

namespace {

std::wstring Utf8ToWide(const std::string& value) {
  if (value.empty()) return {};
  const int length = MultiByteToWideChar(CP_UTF8, 0, value.data(),
                                         static_cast<int>(value.size()),
                                         nullptr, 0);
  if (length <= 0) return {};
  std::wstring result(static_cast<size_t>(length), L'\0');
  MultiByteToWideChar(CP_UTF8, 0, value.data(),
                      static_cast<int>(value.size()), result.data(), length);
  return result;
}

std::string WideToUtf8(const std::wstring& value) {
  if (value.empty()) return {};
  const int length = WideCharToMultiByte(CP_UTF8, 0, value.data(),
                                        static_cast<int>(value.size()), nullptr,
                                        0, nullptr, nullptr);
  if (length <= 0) return {};
  std::string result(static_cast<size_t>(length), '\0');
  WideCharToMultiByte(CP_UTF8, 0, value.data(),
                      static_cast<int>(value.size()), result.data(), length,
                      nullptr, nullptr);
  return result;
}

HWND TopLevelWindow(HWND window) {
  if (window == nullptr || !IsWindow(window)) return nullptr;
  HWND root = GetAncestor(window, GA_ROOT);
  return root == nullptr ? window : root;
}

bool BelongsToCurrentProcess(HWND window) {
  window = TopLevelWindow(window);
  if (window == nullptr) return false;
  DWORD process_id = 0;
  GetWindowThreadProcessId(window, &process_id);
  return process_id != 0 && process_id == GetCurrentProcessId();
}

bool IsShellWindow(HWND window) {
  window = TopLevelWindow(window);
  if (window == nullptr) return false;

  wchar_t class_name[256] = {};
  GetClassNameW(window, class_name, static_cast<int>(std::size(class_name)));
  const std::wstring name = class_name;

  // Clicking a notification-area icon can transiently make the taskbar or
  // its overflow popup the foreground window. Those windows have no input
  // field and must never become a sticker send target. Keep this list limited
  // to shell surfaces so normal Explorer document windows remain usable.
  return name == L"Shell_TrayWnd" || name == L"Shell_SecondaryTrayWnd" ||
         name == L"NotifyIconOverflowWindow" ||
         name == L"TopLevelWindowForOverflowXamlIsland" ||
         name == L"Xaml_WindowedPopupClass" || name == L"Progman" ||
         name == L"WorkerW" || name == L"TaskListThumbnailWnd";
}

HWINEVENTHOOK g_foreground_hook = nullptr;
HWND g_last_external_window = nullptr;
ULONGLONG g_last_external_tick = 0;
// WM_HOTKEY reaches this window before hotkey_manager forwards the event to
// Dart. Keep a one-shot snapshot so the asynchronous Dart callback cannot
// lose the window that was active when the key was pressed.
HWND g_pending_external_window = nullptr;
ULONGLONG g_pending_external_tick = 0;
bool g_pending_external_capture = false;
DWORD g_last_external_process_id = 0;
std::wstring g_last_external_class;
std::wstring g_last_external_title;
// These fields belong to the one-shot send target. They must not be updated
// by later foreground events while the picker is open.
DWORD g_captured_process_id = 0;
std::wstring g_captured_class;
std::wstring g_captured_title;
HWND g_captured_window = nullptr;
// Keep the control that had focus when the target was captured. Chromium based
// clients (including QQNT) often activate their top-level window without
// restoring the editor control automatically.
HWND g_captured_focus_window = nullptr;
ULONGLONG g_captured_tick = 0;
// Every frozen target receives a monotonically increasing token. Dart sends
// the token back when it performs asynchronous cleanup, so an old cleanup
// cannot clear a newer capture that happens to reuse the same HWND.
ULONGLONG g_next_captured_generation = 0;
ULONGLONG g_captured_generation = 0;
// The native side also tracks the quick-picker session. RegisterHotKey can
// emit another WM_HOTKEY while the picker is still visible; such an event
// must never replace the target captured for the active session.
bool g_quick_picker_active = false;
// Keep the last external foreground window long enough for a user to switch
// from the manager to QQ/WeChat and back before choosing a sticker. The handle
// and frozen process/class identity are still validated at activation time.
constexpr ULONGLONG kExternalWindowFreshnessMs = 30000;
// A busy Flutter isolate can take several seconds to consume the native
// snapshot while a large import is finishing. The HWND and frozen identity are
// still validated at activation time, so keep this one-shot token alive long
// enough for that hand-off without making it a permanent target.
constexpr ULONGLONG kPendingExternalWindowFreshnessMs = 30000;
constexpr ULONGLONG kHotKeySnapshotCoalesceMs = 800;
constexpr ULONGLONG kTraySnapshotCoalesceMs = 800;
constexpr UINT kTrayCallbackMessage = WM_USER + 1;
constexpr WPARAM kHotKeyAvailabilityProbeId = 0x7ffe;
ULONGLONG g_last_hotkey_snapshot_tick = 0;
WPARAM g_last_hotkey_snapshot_id = 0;
ULONGLONG g_last_tray_snapshot_tick = 0;

bool MatchesCapturedWindowIdentity(HWND window);
bool ActivateWindow(HWND target);

void ClearExternalWindowMetadata() {
  g_last_external_process_id = 0;
  g_last_external_class.clear();
  g_last_external_title.clear();
}

void ClearRecentExternalWindow() {
  g_last_external_window = nullptr;
  g_last_external_tick = 0;
  ClearExternalWindowMetadata();
}

void ClearCapturedTargetMetadata() {
  g_captured_process_id = 0;
  g_captured_class.clear();
  g_captured_title.clear();
  g_captured_window = nullptr;
  g_captured_focus_window = nullptr;
  g_captured_tick = 0;
  g_captured_generation = 0;
}

void RememberExternalWindow(HWND window) {
  window = TopLevelWindow(window);
  if (window == nullptr || BelongsToCurrentProcess(window) ||
      IsShellWindow(window)) {
    return;
  }

  DWORD process_id = 0;
  GetWindowThreadProcessId(window, &process_id);
  if (process_id == 0) return;

  wchar_t class_name[256] = {};
  wchar_t title[512] = {};
  GetClassNameW(window, class_name, static_cast<int>(std::size(class_name)));
  GetWindowTextW(window, title, static_cast<int>(std::size(title)));
  g_last_external_process_id = process_id;
  g_last_external_class = class_name;
  g_last_external_title = title;
}

void FreezeCapturedTarget(HWND window) {
  window = TopLevelWindow(window);
  if (window == nullptr || BelongsToCurrentProcess(window) ||
      IsShellWindow(window)) {
    ClearCapturedTargetMetadata();
    return;
  }

  DWORD process_id = 0;
  GetWindowThreadProcessId(window, &process_id);
  if (process_id == 0) {
    ClearCapturedTargetMetadata();
    return;
  }

  wchar_t class_name[256] = {};
  wchar_t title[512] = {};
  GetClassNameW(window, class_name, static_cast<int>(std::size(class_name)));
  GetWindowTextW(window, title, static_cast<int>(std::size(title)));
  g_captured_process_id = process_id;
  g_captured_class = class_name;
  g_captured_title = title;
  g_captured_window = window;
  g_captured_tick = GetTickCount64();
  g_captured_generation = ++g_next_captured_generation;
  g_captured_focus_window = nullptr;
  GUITHREADINFO thread_info{};
  thread_info.cbSize = sizeof(thread_info);
  const DWORD thread_id = GetWindowThreadProcessId(window, nullptr);
  if (thread_id != 0 && GetGUIThreadInfo(thread_id, &thread_info)) {
    const auto focus = thread_info.hwndFocus != nullptr
                           ? thread_info.hwndFocus
                           : thread_info.hwndActive;
    if (focus != nullptr && TopLevelWindow(focus) == window) {
      g_captured_focus_window = focus;
    }
  }
}

void CALLBACK ForegroundWindowChanged(HWINEVENTHOOK, DWORD event, HWND window,
                                      LONG, LONG, DWORD, DWORD) {
  if (event != EVENT_SYSTEM_FOREGROUND) return;
  window = TopLevelWindow(window);
  if (window != nullptr && !BelongsToCurrentProcess(window) &&
      !IsShellWindow(window)) {
    g_last_external_window = window;
    g_last_external_tick = GetTickCount64();
    RememberExternalWindow(window);
  }
}

HWND RecentExternalWindow() {
  auto candidate = TopLevelWindow(g_last_external_window);
  bool identity_matches = false;
  if (candidate != nullptr && !BelongsToCurrentProcess(candidate) &&
      !IsShellWindow(candidate) && g_last_external_process_id != 0) {
    DWORD process_id = 0;
    GetWindowThreadProcessId(candidate, &process_id);
    identity_matches = process_id == g_last_external_process_id;
    if (identity_matches && !g_last_external_class.empty()) {
      wchar_t class_name[256] = {};
      GetClassNameW(candidate, class_name,
                    static_cast<int>(std::size(class_name)));
      identity_matches = g_last_external_class == class_name;
    }
  }
  if (identity_matches && g_last_external_tick != 0 &&
      GetTickCount64() - g_last_external_tick <=
          kExternalWindowFreshnessMs) {
    return candidate;
  }
  ClearRecentExternalWindow();
  return nullptr;
}

void ClearPendingExternalWindow() {
  g_pending_external_window = nullptr;
  g_pending_external_tick = 0;
  g_pending_external_capture = false;
}

void CaptureExternalWindowSnapshot(bool preserve_fresh_snapshot = false) {
  const auto now = GetTickCount64();
  // A hotkey snapshot is already in flight. A tray callback arriving during
  // that hand-off must not replace it with the shell/taskbar window.
  if (preserve_fresh_snapshot && g_pending_external_capture &&
      g_pending_external_tick != 0 &&
      now - g_pending_external_tick <= kPendingExternalWindowFreshnessMs) {
    return;
  }
  if (preserve_fresh_snapshot && g_last_tray_snapshot_tick != 0 &&
      now - g_last_tray_snapshot_tick <= kTraySnapshotCoalesceMs) {
    return;
  }
  ClearPendingExternalWindow();
  ClearCapturedTargetMetadata();

  const auto foreground = TopLevelWindow(GetForegroundWindow());
  if (foreground != nullptr && !BelongsToCurrentProcess(foreground) &&
      !IsShellWindow(foreground)) {
    if (preserve_fresh_snapshot) g_last_tray_snapshot_tick = now;
    g_pending_external_capture = true;
    g_pending_external_window = foreground;
    g_pending_external_tick = now;
    g_last_external_window = foreground;
    g_last_external_tick = g_pending_external_tick;
    RememberExternalWindow(foreground);
    FreezeCapturedTarget(foreground);
  } else {
    // The desktop can briefly report no foreground window during a task
    // switch. The foreground hook's recent external snapshot covers that
    // narrow transition without guessing from Z-order.
    const auto recent = RecentExternalWindow();
    if (recent != nullptr) {
      if (preserve_fresh_snapshot) g_last_tray_snapshot_tick = now;
      g_pending_external_capture = true;
      g_pending_external_window = recent;
      g_pending_external_tick = now;
      RememberExternalWindow(recent);
      FreezeCapturedTarget(recent);
    } else if (preserve_fresh_snapshot) {
      // Do not suppress the next tray message when this one only observed a
      // shell surface; the next message may see the actual external window.
      g_last_tray_snapshot_tick = 0;
    }
  }
}

HWND CaptureExternalWindow(bool consume_pending, bool allow_recent_manual) {
  if (consume_pending && g_pending_external_capture) {
    const auto candidate = TopLevelWindow(g_pending_external_window);
    const auto captured_at = g_pending_external_tick;
    ClearPendingExternalWindow();
    if (candidate != nullptr && !BelongsToCurrentProcess(candidate) &&
        !IsShellWindow(candidate) &&
        captured_at != 0 &&
        GetTickCount64() - captured_at <=
            kPendingExternalWindowFreshnessMs) {
      return candidate;
    }

    // The pending HWND can disappear while QQ/QQNT rebuilds its window. Keep
    // the frozen identity as a short-lived fallback before giving up, then let
    // the common foreground/recent path try once more.
    const auto frozen = TopLevelWindow(g_captured_window);
    if (frozen != nullptr && !BelongsToCurrentProcess(frozen) &&
        !IsShellWindow(frozen) && g_captured_tick != 0 &&
        GetTickCount64() - g_captured_tick <=
            kPendingExternalWindowFreshnessMs &&
        MatchesCapturedWindowIdentity(frozen)) {
      return frozen;
    }
    ClearCapturedTargetMetadata();
  }

  // A manual picker open must never inherit an expired target captured by a
  // previous hotkey event. The toolbar's quick-picker action may explicitly
  // opt into the short-lived foreground snapshot so that clicking the button
  // immediately after leaving QQ/WeChat still has a useful send target.
  if (!consume_pending) {
    ClearPendingExternalWindow();
    ClearCapturedTargetMetadata();
  }

  const auto foreground = TopLevelWindow(GetForegroundWindow());
  if (foreground != nullptr && BelongsToCurrentProcess(foreground)) {
    if (allow_recent_manual) {
      const auto recent = RecentExternalWindow();
      if (recent != nullptr) {
        FreezeCapturedTarget(recent);
        return recent;
      }
    }
    return nullptr;
  }
  if (foreground != nullptr && !IsShellWindow(foreground)) {
    g_last_external_window = foreground;
    g_last_external_tick = GetTickCount64();
    RememberExternalWindow(foreground);
    FreezeCapturedTarget(foreground);
    return foreground;
  }
  if (allow_recent_manual) {
    const auto recent = RecentExternalWindow();
    if (recent != nullptr) {
      FreezeCapturedTarget(recent);
      return recent;
    }
  }
  return nullptr;
}

struct ExternalWindowSearch {
  DWORD process_id = 0;
  const std::wstring* class_name = nullptr;
  const std::wstring* title = nullptr;
  bool require_class = false;
  bool require_title = false;
  HWND best = nullptr;
  int best_score = -1;
  int candidate_count = 0;
};

BOOL CALLBACK FindExternalWindowCallback(HWND window, LPARAM parameter) {
  auto* search = reinterpret_cast<ExternalWindowSearch*>(parameter);
  if (search == nullptr || !IsWindowVisible(window)) return TRUE;

  DWORD process_id = 0;
  GetWindowThreadProcessId(window, &process_id);
  if (process_id == 0 || process_id != search->process_id ||
      BelongsToCurrentProcess(window)) {
    return TRUE;
  }

  wchar_t class_name[256] = {};
  wchar_t title[512] = {};
  GetClassNameW(window, class_name, static_cast<int>(std::size(class_name)));
  GetWindowTextW(window, title, static_cast<int>(std::size(title)));
  const bool class_match = search->class_name != nullptr &&
                           !search->class_name->empty() &&
                           *search->class_name == class_name;
  const bool title_match = search->title != nullptr &&
                           !search->title->empty() &&
                           *search->title == title;
  if (search->require_class && !class_match) return TRUE;
  if (search->require_title && !title_match) return TRUE;

  search->candidate_count++;
  const bool unowned = GetWindow(window, GW_OWNER) == nullptr;
  const int score = (class_match ? 8 : 0) + (title_match ? 4 : 0) +
                    (unowned ? 2 : 0) + (IsWindowEnabled(window) ? 1 : 0);
  if (score > search->best_score) {
    search->best = window;
    search->best_score = score;
  }
  return TRUE;
}

HWND FindReplacementExternalWindow() {
  if (g_captured_process_id == 0) return nullptr;
  const bool has_class = !g_captured_class.empty();
  const bool has_title = !g_captured_title.empty();

  // First require every piece of frozen identity that was available at
  // capture time. This is the only safe path when QQ/QQNT owns multiple
  // visible top-level windows in one process.
  ExternalWindowSearch exact{g_captured_process_id,
                             &g_captured_class,
                             &g_captured_title,
                             has_class,
                             has_title};
  EnumWindows(FindExternalWindowCallback, reinterpret_cast<LPARAM>(&exact));
  if (exact.candidate_count == 1) return exact.best;

  // Titles can change while a chat window is being rebuilt. Permit a title
  // mismatch only when the captured class identifies exactly one visible
  // window in the original process; never pick an arbitrary sibling window.
  ExternalWindowSearch class_only{g_captured_process_id,
                                  &g_captured_class,
                                  &g_captured_title,
                                  has_class,
                                  false};
  EnumWindows(FindExternalWindowCallback,
              reinterpret_cast<LPARAM>(&class_only));
  if (class_only.candidate_count == 1) return class_only.best;
  return nullptr;
}

bool MatchesCapturedWindowIdentity(HWND window) {
  window = TopLevelWindow(window);
  if (window == nullptr) return false;

  DWORD process_id = 0;
  GetWindowThreadProcessId(window, &process_id);
  if (g_captured_process_id == 0 || process_id != g_captured_process_id) {
    return false;
  }

  if (!g_captured_class.empty()) {
    wchar_t class_name[256] = {};
    GetClassNameW(window, class_name, static_cast<int>(std::size(class_name)));
    if (g_captured_class != class_name) return false;
  }

  // The original HWND may legitimately keep its identity while a chat title
  // changes. A different HWND must retain the frozen title; otherwise it is
  // only accepted through the unique-candidate path in
  // FindReplacementExternalWindow().
  if (window != g_captured_window && !g_captured_title.empty()) {
    wchar_t title[512] = {};
    GetWindowTextW(window, title, static_cast<int>(std::size(title)));
    if (g_captured_title != title) return false;
  }
  return true;
}

HWND ResolveExternalWindow(HWND candidate) {
  candidate = TopLevelWindow(candidate);
  if (candidate != nullptr && !BelongsToCurrentProcess(candidate)) {
    DWORD process_id = 0;
    GetWindowThreadProcessId(candidate, &process_id);
    if (g_captured_process_id != 0 &&
        process_id == g_captured_process_id && !IsShellWindow(candidate) &&
        MatchesCapturedWindowIdentity(candidate)) {
      return candidate;
    }
  }
  return FindReplacementExternalWindow();
}

std::string WindowProcessName(HWND window) {
  window = TopLevelWindow(window);
  if (window == nullptr) return {};
  DWORD process_id = 0;
  GetWindowThreadProcessId(window, &process_id);
  if (process_id == 0) return {};
  HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE,
                               process_id);
  if (process == nullptr) return {};
  std::wstring executable_path(32768, L'\0');
  DWORD length = static_cast<DWORD>(executable_path.size());
  const BOOL queried = QueryFullProcessImageNameW(
      process, 0, executable_path.data(), &length);
  CloseHandle(process);
  if (!queried || length == 0) return {};
  executable_path.resize(length);
  const auto separator = executable_path.find_last_of(L"\\/");
  const auto executable_name = separator == std::wstring::npos
                                   ? executable_path
                                   : executable_path.substr(separator + 1);
  return WideToUtf8(executable_name);
}

bool OpenClipboardWithRetry() {
  // QQ/WeChat can keep the clipboard open briefly while it snapshots the
  // previous item. A single short attempt makes a normal click look like a
  // copy failure, so give the owner a bounded hand-off window.
  constexpr int kAttempts = 24;
  constexpr DWORD kRetryDelayMs = 20;
  for (int attempt = 0; attempt < kAttempts; ++attempt) {
    if (OpenClipboard(nullptr)) return true;
    Sleep(kRetryDelayMs);
  }
  return false;
}

bool RestoreCapturedFocus(HWND target) {
  const auto focus = g_captured_focus_window;
  target = TopLevelWindow(target);
  if (focus == nullptr || target == nullptr || !IsWindow(focus) ||
      TopLevelWindow(focus) != target) {
    return false;
  }

  const DWORD current_thread = GetCurrentThreadId();
  const DWORD target_thread = GetWindowThreadProcessId(focus, nullptr);
  bool attached = false;
  if (target_thread != 0 && target_thread != current_thread) {
    attached = AttachThreadInput(current_thread, target_thread, TRUE) != FALSE;
  }
  SetFocus(focus);
  const bool focused = GetFocus() == focus;
  if (attached) AttachThreadInput(current_thread, target_thread, FALSE);
  return focused;
}

bool EnsureCapturedTargetForeground() {
  auto captured = TopLevelWindow(g_captured_window);
  if (captured == nullptr || BelongsToCurrentProcess(captured) ||
      IsShellWindow(captured) || !MatchesCapturedWindowIdentity(captured)) {
    // This path is only used by the native send methods. Refuse to inject
    // input when no external target was captured instead of sending it to the
    // manager window that currently owns the foreground.
    captured = ResolveExternalWindow(captured);
    if (captured == nullptr) return false;
    g_captured_window = captured;
  }

  const auto foreground = TopLevelWindow(GetForegroundWindow());
  if (foreground != captured) {
    if (!ActivateWindow(captured)) return false;
    captured = TopLevelWindow(g_captured_window);
  } else {
    // The top-level window can remain foreground while its renderer/editor
    // control loses focus after an asynchronous paste. Restore the control
    // captured before the manager took focus when it is still valid.
    RestoreCapturedFocus(captured);
  }
  return captured != nullptr && MatchesCapturedWindowIdentity(captured) &&
         TopLevelWindow(GetForegroundWindow()) == captured;
}

HGLOBAL CreateFileDrop(const std::wstring& file_path) {
  const SIZE_T bytes = sizeof(DROPFILES) +
                       (file_path.size() + 1 + 1) * sizeof(wchar_t);
  HGLOBAL storage = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes);
  if (!storage) return nullptr;
  auto* drop = static_cast<DROPFILES*>(GlobalLock(storage));
  if (!drop) {
    GlobalFree(storage);
    return nullptr;
  }
  drop->pFiles = sizeof(DROPFILES);
  drop->fWide = TRUE;
  auto* output = reinterpret_cast<wchar_t*>(reinterpret_cast<BYTE*>(drop) +
                                            sizeof(DROPFILES));
  std::memcpy(output, file_path.c_str(), file_path.size() * sizeof(wchar_t));
  GlobalUnlock(storage);
  return storage;
}

HGLOBAL CreateDibFromBitmap(HBITMAP bitmap) {
  if (bitmap == nullptr) return nullptr;

  // GetHBITMAP is allowed to return either a device-dependent bitmap (DDB)
  // or a DIB section. Asking GetObject for DIBSECTION therefore rejects valid
  // DDBs on some Windows/GDI+ combinations. BITMAP is the common contract for
  // both kinds and gives us the dimensions needed for a 32-bit CF_DIB.
  BITMAP source{};
  if (GetObjectW(bitmap, sizeof(source), &source) != sizeof(source)) {
    return nullptr;
  }
  const LONG width = source.bmWidth;
  const LONG height = source.bmHeight;
  if (width <= 0 || height <= 0) return nullptr;

  const auto width_size = static_cast<SIZE_T>(width);
  const auto height_size = static_cast<SIZE_T>(height);
  constexpr SIZE_T kBitsPerPixel = 32;
  if (width_size >
      (std::numeric_limits<DWORD>::max() - 31u) / kBitsPerPixel) {
    return nullptr;
  }
  const DWORD stride =
      ((static_cast<DWORD>(width_size) * kBitsPerPixel + 31u) / 32u) * 4u;
  if (height_size > std::numeric_limits<SIZE_T>::max() / stride) {
    return nullptr;
  }
  const SIZE_T image_size = static_cast<SIZE_T>(stride) * height_size;
  if (image_size > std::numeric_limits<DWORD>::max() -
                       sizeof(BITMAPINFOHEADER)) {
    return nullptr;
  }
  const SIZE_T total_size = sizeof(BITMAPINFOHEADER) + image_size;
  HGLOBAL storage = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total_size);
  if (!storage) return nullptr;
  auto* header = static_cast<BITMAPINFOHEADER*>(GlobalLock(storage));
  if (!header) {
    GlobalFree(storage);
    return nullptr;
  }
  header->biSize = sizeof(BITMAPINFOHEADER);
  header->biWidth = width;
  header->biHeight = height;
  header->biPlanes = 1;
  header->biBitCount = 32;
  header->biCompression = BI_RGB;
  header->biSizeImage = static_cast<DWORD>(image_size);
  auto* bits = reinterpret_cast<BYTE*>(header) + sizeof(BITMAPINFOHEADER);
  HDC screen = GetDC(nullptr);
  if (screen == nullptr) {
    GlobalUnlock(storage);
    GlobalFree(storage);
    return nullptr;
  }
  const int copied = GetDIBits(screen, bitmap, 0, static_cast<UINT>(height),
                               bits, reinterpret_cast<BITMAPINFO*>(header),
                               DIB_RGB_COLORS);
  ReleaseDC(nullptr, screen);
  GlobalUnlock(storage);
  if (copied != height) {
    GlobalFree(storage);
    return nullptr;
  }
  return storage;
}

HBITMAP CloneBitmapForClipboard(HBITMAP bitmap) {
  if (bitmap == nullptr) return nullptr;

  // GDI+ can return a DIB section whose lifetime is tied to the source
  // image. Give the clipboard an independent bitmap handle so it can take
  // ownership without depending on the GDI+ image teardown below.
  return static_cast<HBITMAP>(
      CopyImage(bitmap, IMAGE_BITMAP, 0, 0, LR_CREATEDIBSECTION));
}

bool CopyImageToClipboard(const std::wstring& file_path) {
  Gdiplus::GdiplusStartupInput startup_input;
  ULONG_PTR token = 0;
  if (Gdiplus::GdiplusStartup(&token, &startup_input, nullptr) !=
      Gdiplus::Ok) {
    return false;
  }
  auto* image = Gdiplus::Bitmap::FromFile(file_path.c_str(), FALSE);
  if (!image || image->GetLastStatus() != Gdiplus::Ok) {
    delete image;
    Gdiplus::GdiplusShutdown(token);
    return false;
  }
  HBITMAP bitmap = nullptr;
  const auto bitmap_status = image->GetHBITMAP(Gdiplus::Color(0, 0, 0, 0),
                                               &bitmap);
  HGLOBAL dib = bitmap_status == Gdiplus::Ok
                    ? CreateDibFromBitmap(bitmap)
                    : nullptr;
  HBITMAP clipboard_bitmap = CloneBitmapForClipboard(bitmap);
  bool copied = false;
  if (bitmap && OpenClipboardWithRetry()) {
    if (EmptyClipboard()) {
      // Prefer CF_BITMAP. Windows clients that only understand the legacy
      // bitmap format can paste it directly, and Windows also synthesizes
      // CF_DIB/CF_DIBV5 from a successful CF_BITMAP write.
      if (clipboard_bitmap != nullptr &&
          SetClipboardData(CF_BITMAP, clipboard_bitmap) != nullptr) {
        clipboard_bitmap = nullptr;
        copied = true;
      }
      // If cloning failed, try the GDI+ handle itself. This fallback is safe
      // because ownership is transferred only when SetClipboardData succeeds.
      if (!copied && SetClipboardData(CF_BITMAP, bitmap) != nullptr) {
        bitmap = nullptr;
        copied = true;
      }
      // Keep an explicit CF_DIB representation for clients that prefer raw
      // DIB data. It is independent of the bitmap handle above.
      if (dib && SetClipboardData(CF_DIB, dib) != nullptr) {
        dib = nullptr;
        copied = true;
      }
    }
    CloseClipboard();
  }
  if (clipboard_bitmap) DeleteObject(clipboard_bitmap);
  if (dib) GlobalFree(dib);
  if (bitmap) DeleteObject(bitmap);
  delete image;
  Gdiplus::GdiplusShutdown(token);
  return copied;
}

bool CopyGifToClipboard(const std::wstring& file_path) {
  if (!OpenClipboardWithRetry()) return false;
  EmptyClipboard();
  HGLOBAL storage = CreateFileDrop(file_path);
  bool copied = storage != nullptr && SetClipboardData(CF_HDROP, storage) != nullptr;
  if (!copied && storage) GlobalFree(storage);
  CloseClipboard();
  return copied;
}

bool SendPasteShortcut() {
  if (!EnsureCapturedTargetForeground()) return false;
  INPUT inputs[4]{};
  inputs[0].type = INPUT_KEYBOARD;
  inputs[0].ki.wVk = VK_CONTROL;
  inputs[1].type = INPUT_KEYBOARD;
  inputs[1].ki.wVk = 'V';
  inputs[2] = inputs[1];
  inputs[2].ki.dwFlags = KEYEVENTF_KEYUP;
  inputs[3] = inputs[0];
  inputs[3].ki.dwFlags = KEYEVENTF_KEYUP;
  return SendInput(4, inputs, sizeof(INPUT)) == 4;
}

bool SendEnter() {
  if (!EnsureCapturedTargetForeground()) return false;
  INPUT inputs[2]{};
  inputs[0].type = INPUT_KEYBOARD;
  inputs[0].ki.wVk = VK_RETURN;
  inputs[1] = inputs[0];
  inputs[1].ki.dwFlags = KEYEVENTF_KEYUP;
  return SendInput(2, inputs, sizeof(INPUT)) == 2;
}

bool ActivateWindow(HWND target) {
  target = ResolveExternalWindow(target);
  if (target == nullptr) {
    g_quick_picker_active = false;
    ClearRecentExternalWindow();
    ClearCapturedTargetMetadata();
    return false;
  }
  // ResolveExternalWindow may replace a destroyed HWND with the unique live
  // window that still matches the frozen process/class identity. Keep the
  // resolved handle for the subsequent paste and Enter calls.
  g_captured_window = target;
  if (IsIconic(target)) ShowWindow(target, SW_RESTORE);
  ShowWindow(target, SW_SHOW);

  const DWORD current_thread = GetCurrentThreadId();
  HWND foreground_window = TopLevelWindow(GetForegroundWindow());
  const DWORD foreground_thread =
      foreground_window == nullptr
          ? 0
          : GetWindowThreadProcessId(foreground_window, nullptr);
  DWORD target_thread = GetWindowThreadProcessId(target, nullptr);
  bool attached_foreground = false;
  bool attached_target = false;
  if (foreground_thread != 0 && foreground_thread != current_thread &&
      foreground_thread != target_thread) {
    attached_foreground =
        AttachThreadInput(current_thread, foreground_thread, TRUE) != FALSE;
  }
  if (target_thread != 0 && target_thread != current_thread) {
    attached_target =
        AttachThreadInput(current_thread, target_thread, TRUE) != FALSE;
  }
  // QQNT and other Chromium windows may complete activation asynchronously.
  // Retry briefly while the input queues are attached, and verify the actual
  // foreground handle before allowing SendInput to run.
  for (int attempt = 0; attempt < 12; ++attempt) {
    if (!IsWindow(target)) {
      if (attached_target) {
        AttachThreadInput(current_thread, target_thread, FALSE);
        attached_target = false;
      }
      target = FindReplacementExternalWindow();
      if (target == nullptr) break;
      target_thread = GetWindowThreadProcessId(target, nullptr);
      if (target_thread != 0 && target_thread != current_thread) {
        attached_target =
            AttachThreadInput(current_thread, target_thread, TRUE) != FALSE;
      }
    }
    BringWindowToTop(target);
    SetActiveWindow(target);
    SetForegroundWindow(target);
    if (TopLevelWindow(GetForegroundWindow()) == target) break;
    Sleep(15);
  }

  // Restore the editor/control that was focused at capture time while the
  // target input queue is still attached. If QQ/QQNT rebuilt that child
  // window, the helper safely falls back to the activated root.
  if (target != nullptr && TopLevelWindow(GetForegroundWindow()) == target) {
    RestoreCapturedFocus(target);
  }
  if (attached_target) {
    AttachThreadInput(current_thread, target_thread, FALSE);
  }
  if (attached_foreground) {
    AttachThreadInput(current_thread, foreground_thread, FALSE);
  }
  // SetForegroundWindow may return FALSE while the foreground transition is
  // completed asynchronously. The actual foreground handle is authoritative.
  const bool activated =
      target != nullptr && TopLevelWindow(GetForegroundWindow()) == target;
  g_quick_picker_active = false;
  if (!activated) {
    // A failed activation must not leave a stale native target around for a
    // later click. On success the target is deliberately retained until Dart
    // finishes the paste/send sequence and calls clearExternalWindowTarget.
    ClearRecentExternalWindow();
    ClearCapturedTargetMetadata();
  }
  return activated;
}

bool IsHotKeyAvailable(const flutter::EncodableValue* arguments,
                       HWND window) {
  const auto* values = std::get_if<flutter::EncodableMap>(arguments);
  if (values == nullptr) return false;

  int key_code = 0;
  const auto key_it = values->find(flutter::EncodableValue("keyCode"));
  if (key_it == values->end() ||
      !std::holds_alternative<int>(key_it->second)) {
    return false;
  }
  key_code = std::get<int>(key_it->second);
  if (key_code <= 0 || key_code > 0xff) return false;

  UINT modifiers = 0;
  const auto modifiers_it = values->find(flutter::EncodableValue("modifiers"));
  if (modifiers_it != values->end()) {
    const auto* modifier_values =
        std::get_if<flutter::EncodableList>(&modifiers_it->second);
    if (modifier_values == nullptr) return false;
    for (const auto& value : *modifier_values) {
      if (!std::holds_alternative<std::string>(value)) return false;
      const auto& modifier = std::get<std::string>(value);
      if (modifier == "alt") {
        modifiers |= MOD_ALT;
      } else if (modifier == "control") {
        modifiers |= MOD_CONTROL;
      } else if (modifier == "meta") {
        modifiers |= MOD_WIN;
      } else if (modifier == "shift") {
        modifiers |= MOD_SHIFT;
      } else {
        // RegisterHotKey has no equivalent for caps-lock or fn modifiers.
        return false;
      }
    }
  }
  if (modifiers == 0) return false;

  constexpr int kProbeHotKeyId = 0x7ffe;
  if (!RegisterHotKey(window, kProbeHotKeyId, modifiers,
                      static_cast<UINT>(key_code))) {
    return false;
  }
  UnregisterHotKey(window, kProbeHotKeyId);
  return true;
}

}  // namespace

FlutterWindow::FlutterWindow(const flutter::DartProject& project)
    : project_(project) {}

FlutterWindow::~FlutterWindow() {}

bool FlutterWindow::OnCreate() {
  if (!Win32Window::OnCreate()) {
    return false;
  }

  RECT frame = GetClientArea();

  // The size here must match the window dimensions to avoid unnecessary surface
  // creation / destruction in the startup path.
  flutter_controller_ = std::make_unique<flutter::FlutterViewController>(
      frame.right - frame.left, frame.bottom - frame.top, project_);
  // Ensure that basic setup of the controller was successful.
  if (!flutter_controller_->engine() || !flutter_controller_->view()) {
    return false;
  }
  RegisterPlugins(flutter_controller_->engine());
  g_foreground_hook = SetWinEventHook(
      EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, nullptr,
      ForegroundWindowChanged, 0, 0, WINEVENT_OUTOFCONTEXT);
  platform_channel_ = std::make_unique<
      flutter::MethodChannel<flutter::EncodableValue>>(
      flutter_controller_->engine()->messenger(), "sticker_manager/platform",
      &flutter::StandardMethodCodec::GetInstance());
  platform_channel_->SetMethodCallHandler(
      [this](const flutter::MethodCall<flutter::EncodableValue>& call,
             std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>>
                 result) {
        if (call.method_name() == "getWindowHandle") {
          result->Success(flutter::EncodableValue(
              static_cast<int64_t>(reinterpret_cast<intptr_t>(GetHandle()))));
          return;
        }
        if (call.method_name() == "isHotKeyAvailable") {
          result->Success(flutter::EncodableValue(
              IsHotKeyAvailable(call.arguments(), GetHandle())));
          return;
        }
        if (call.method_name() == "isApplicationWindow") {
          int64_t raw_handle = 0;
          if (call.arguments() != nullptr) {
            raw_handle = call.arguments()->TryGetLongValue().value_or(0);
          }
          DWORD process_id = 0;
          const auto handle = TopLevelWindow(reinterpret_cast<HWND>(raw_handle));
          if (handle != nullptr) GetWindowThreadProcessId(handle, &process_id);
          result->Success(flutter::EncodableValue(
              process_id != 0 && process_id == GetCurrentProcessId()));
          return;
        }
        if (call.method_name() == "captureExternalWindow") {
          bool consume_pending = false;
          bool allow_recent_manual = false;
          bool from_hotkey = false;
          if (call.arguments() != nullptr) {
            if (const auto* values =
                    std::get_if<flutter::EncodableMap>(call.arguments())) {
              const auto pending_it =
                  values->find(flutter::EncodableValue("fromHotKey"));
              if (pending_it != values->end() &&
                  std::holds_alternative<bool>(pending_it->second)) {
                from_hotkey = std::get<bool>(pending_it->second);
                consume_pending = from_hotkey;
              }
              const auto consume_it =
                  values->find(flutter::EncodableValue("consumePending"));
              if (consume_it != values->end() &&
                  std::holds_alternative<bool>(consume_it->second)) {
                consume_pending = std::get<bool>(consume_it->second);
              }
              const auto recent_it =
                  values->find(flutter::EncodableValue("allowRecentManual"));
              if (recent_it != values->end() &&
                  std::holds_alternative<bool>(recent_it->second)) {
                allow_recent_manual = std::get<bool>(recent_it->second);
              }
            }
          }
          // A non-hotkey capture is an explicit management/tray entry point;
          // it ends the native quick-picker session while preserving the
          // newly captured management target.
          if (!from_hotkey) g_quick_picker_active = false;
          const auto external =
              CaptureExternalWindow(consume_pending, allow_recent_manual);
          if (external == nullptr) {
            result->Success(flutter::EncodableValue(static_cast<int64_t>(0)));
            return;
          }
          flutter::EncodableMap payload;
          payload[flutter::EncodableValue("handle")] =
              flutter::EncodableValue(static_cast<int64_t>(
                  reinterpret_cast<intptr_t>(external)));
          payload[flutter::EncodableValue("generation")] =
              flutter::EncodableValue(static_cast<int64_t>(
                  g_captured_generation));
          result->Success(flutter::EncodableValue(payload));
          return;
        }
        if (call.method_name() == "setQuickPickerActive") {
          bool active = false;
          if (call.arguments() != nullptr) {
            if (const auto* values =
                    std::get_if<flutter::EncodableMap>(call.arguments())) {
              const auto active_it =
                  values->find(flutter::EncodableValue("active"));
              if (active_it != values->end() &&
                  std::holds_alternative<bool>(active_it->second)) {
                active = std::get<bool>(active_it->second);
              }
            }
          }
          g_quick_picker_active = active;
          result->Success();
          return;
        }
        if (call.method_name() == "clearExternalWindowTarget") {
          int64_t expected_handle = 0;
          int64_t expected_generation = 0;
          if (call.arguments() != nullptr) {
            if (const auto* values =
                    std::get_if<flutter::EncodableMap>(call.arguments())) {
              const auto handle_it =
                  values->find(flutter::EncodableValue("expectedHandle"));
              if (handle_it != values->end()) {
                if (std::holds_alternative<int64_t>(handle_it->second)) {
                  expected_handle = std::get<int64_t>(handle_it->second);
                } else if (std::holds_alternative<int>(handle_it->second)) {
                  expected_handle = std::get<int>(handle_it->second);
                }
              }
              const auto generation_it =
                  values->find(flutter::EncodableValue("expectedGeneration"));
              if (generation_it != values->end()) {
                if (std::holds_alternative<int64_t>(generation_it->second)) {
                  expected_generation =
                      std::get<int64_t>(generation_it->second);
                } else if (std::holds_alternative<int>(generation_it->second)) {
                  expected_generation = std::get<int>(generation_it->second);
                }
              }
            }
          }
          // An old asynchronous Dart cleanup must not erase a newer hotkey
          // snapshot. A zero handle is intentionally a no-op. Older Dart
          // callers may omit the generation; retain the handle-only behavior
          // for that compatibility path.
          const bool handle_matches =
              expected_handle != 0 &&
              reinterpret_cast<int64_t>(g_captured_window) == expected_handle;
          const bool generation_matches =
              expected_generation != 0 &&
              static_cast<int64_t>(g_captured_generation) ==
                  expected_generation;
          // A successful activation may replace a destroyed HWND while
          // retaining the same capture generation. In that case the original
          // Dart handle no longer matches, so the generation is authoritative.
          if ((generation_matches ||
               (expected_generation == 0 && handle_matches))) {
            ClearPendingExternalWindow();
            ClearRecentExternalWindow();
            ClearCapturedTargetMetadata();
          }
          result->Success();
          return;
        }
        if (call.method_name() == "getWindowProcessName") {
          int64_t raw_handle = 0;
          if (call.arguments() != nullptr) {
            raw_handle = call.arguments()->TryGetLongValue().value_or(0);
          }
          result->Success(flutter::EncodableValue(WindowProcessName(
              reinterpret_cast<HWND>(raw_handle))));
          return;
        }
        if (call.method_name() == "sendPaste") {
          result->Success(flutter::EncodableValue(SendPasteShortcut()));
          return;
        }
        if (call.method_name() == "sendEnter") {
          result->Success(flutter::EncodableValue(SendEnter()));
          return;
        }
        if (call.method_name() == "activateWindow") {
          int64_t raw_handle = 0;
          if (call.arguments() != nullptr) {
            raw_handle = call.arguments()->TryGetLongValue().value_or(0);
          }
          result->Success(flutter::EncodableValue(ActivateWindow(
              reinterpret_cast<HWND>(raw_handle))));
          return;
        }
        if (call.method_name() == "readClipboardImageData") {
          if (!OpenClipboardWithRetry()) {
            result->Error("clipboard_busy", "无法读取剪贴板，请稍后重试");
            return;
          }
          std::vector<uint8_t> bytes;
          bool too_large = false;
          for (const wchar_t* name : {L"GIF", L"image/gif", L"GIF89a", L"PNG", L"image/png"}) {
            const UINT format = RegisterClipboardFormatW(name);
            if (!IsClipboardFormatAvailable(format)) continue;
            HANDLE handle = GetClipboardData(format);
            if (!handle) continue;
            const SIZE_T size = GlobalSize(handle);
            if (size > 64 * 1024 * 1024) { too_large = true; break; }
            const auto* data = static_cast<const uint8_t*>(GlobalLock(handle));
            if (data && size > 0) bytes.assign(data, data + size);
            if (data) GlobalUnlock(handle);
            if (!bytes.empty()) break;
          }
          CloseClipboard();
          if (too_large) result->Error("too_large", "剪贴板图片超过 64 MiB");
          else if (bytes.empty()) result->Success();
          else result->Success(flutter::EncodableValue(bytes));
          return;
        }
        if (call.method_name() == "copySticker") {
          const auto* arguments =
              std::get_if<flutter::EncodableMap>(call.arguments());
          std::string file_path;
          std::string media_type;
          if (arguments) {
            const auto path_it = arguments->find(flutter::EncodableValue("path"));
            if (path_it != arguments->end() &&
                std::holds_alternative<std::string>(path_it->second)) {
              file_path = std::get<std::string>(path_it->second);
            }
            const auto type_it =
                arguments->find(flutter::EncodableValue("mediaType"));
            if (type_it != arguments->end() &&
                std::holds_alternative<std::string>(type_it->second)) {
              media_type = std::get<std::string>(type_it->second);
            }
          }
          const auto wide_path = Utf8ToWide(file_path);
          const bool copied = !wide_path.empty() &&
                              (media_type == "gif"
                                   ? CopyGifToClipboard(wide_path)
                                   : CopyImageToClipboard(wide_path));
          result->Success(flutter::EncodableValue(copied));
          return;
        }
        result->NotImplemented();
      });
  SetChildContent(flutter_controller_->view()->GetNativeWindow());

  flutter_controller_->engine()->SetNextFrameCallback([&]() {
    this->Show();
  });

  // Flutter can complete the first frame before the "show window" callback is
  // registered. The following call ensures a frame is pending to ensure the
  // window is shown. It is a no-op if the first frame hasn't completed yet.
  flutter_controller_->ForceRedraw();

  return true;
}

void FlutterWindow::OnDestroy() {
  if (g_foreground_hook != nullptr) {
    UnhookWinEvent(g_foreground_hook);
    g_foreground_hook = nullptr;
  }
  g_last_external_window = nullptr;
  g_last_external_tick = 0;
  g_last_hotkey_snapshot_tick = 0;
  g_last_hotkey_snapshot_id = 0;
  g_last_tray_snapshot_tick = 0;
  g_quick_picker_active = false;
  ClearPendingExternalWindow();
  ClearExternalWindowMetadata();
  ClearCapturedTargetMetadata();
  if (platform_channel_) {
    platform_channel_->SetMethodCallHandler(nullptr);
    platform_channel_.reset();
  }
  if (flutter_controller_) {
    flutter_controller_ = nullptr;
  }

  Win32Window::OnDestroy();
}

LRESULT
FlutterWindow::MessageHandler(HWND hwnd, UINT const message,
                              WPARAM const wparam,
                              LPARAM const lparam) noexcept {
  if (message == kRestoreManagementMessage) {
    if (platform_channel_) {
      platform_channel_->InvokeMethod(
          "restoreManagementMode", nullptr);
    }
    ShowWindow(hwnd, SW_SHOW);
    return 0;
  }

  if (message == WM_HOTKEY &&
      static_cast<WPARAM>(wparam) != kHotKeyAvailabilityProbeId) {
    const auto now = GetTickCount64();
    const bool picker_is_visible = IsWindowVisible(hwnd) != FALSE;
    const bool same_hotkey = g_last_hotkey_snapshot_id == wparam;
    bool skip_hotkey_snapshot = false;
    // Once the quick picker has captured a target, every later WM_HOTKEY is
    // stale until the session ends. This guard intentionally outlives the
    // short coalescing window below; otherwise a held key or a second press
    // after 800ms would capture the Flutter window and destroy the QQ target.
    if ((g_quick_picker_active && picker_is_visible) ||
        (picker_is_visible &&
         (g_pending_external_capture || g_captured_window != nullptr))) {
      skip_hotkey_snapshot = true;
    } else if (same_hotkey && g_last_hotkey_snapshot_tick != 0 &&
               now - g_last_hotkey_snapshot_tick <= kHotKeySnapshotCoalesceMs) {
      // RegisterHotKey repeats while the accelerator remains pressed. Do not
      // let a second asynchronous Dart callback consume an empty snapshot and
      // overwrite the target captured for the original key press. Once the
      // picker has been hidden, a new press is a fresh invocation and must be
      // allowed through immediately.
      if (g_pending_external_capture || picker_is_visible) {
        skip_hotkey_snapshot = true;
      }
    }
    if (!skip_hotkey_snapshot) {
      g_last_hotkey_snapshot_tick = now;
      g_last_hotkey_snapshot_id = wparam;
      // Capture before hotkey_manager forwards the event to Dart. Dart then
      // consumes this snapshot after the quick-picker window is shown.
      CaptureExternalWindowSnapshot();
    }
  }

  if (message == kTrayCallbackMessage &&
      (lparam == WM_LBUTTONDOWN || lparam == WM_LBUTTONUP)) {
    // tray_manager forwards the button-up callback to Dart, but the shell may
    // already have moved the foreground to its taskbar window by then. Take
    // the snapshot on button-down when possible and keep button-up as a
    // fallback. A fresh snapshot is coalesced so the two messages produced by
    // a tray double-click cannot overwrite the first one.
    CaptureExternalWindowSnapshot(true);
  }

  if (message == WM_CLOSE) {
    // Keep the process, tray icon, and global hotkey alive while hiding the
    // main window. The tray's explicit exit action still terminates the app.
    ClearPendingExternalWindow();
    ClearRecentExternalWindow();
    ClearCapturedTargetMetadata();
    g_quick_picker_active = false;
    ShowWindow(hwnd, SW_HIDE);
    return 0;
  }

  if (message == WM_SHOWWINDOW && wparam == FALSE) {
    // Hiding the picker ends its native session. This also clears a stale
    // active flag when the window is hidden through the title-bar or tray
    // path before Dart has delivered the mode callback.
    g_quick_picker_active = false;
  }

  // Give Flutter, including plugins, an opportunity to handle window messages.
  if (flutter_controller_) {
    std::optional<LRESULT> result =
        flutter_controller_->HandleTopLevelWindowProc(hwnd, message, wparam,
                                                      lparam);
    if (result) {
      return *result;
    }
  }

  switch (message) {
    case WM_FONTCHANGE:
      flutter_controller_->engine()->ReloadSystemFonts();
      break;
  }

  return Win32Window::MessageHandler(hwnd, message, wparam, lparam);
}
