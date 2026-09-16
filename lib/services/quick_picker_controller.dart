import 'dart:async';
import 'dart:ffi';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/services.dart';
import 'package:hotkey_manager/hotkey_manager.dart';
import 'package:path/path.dart' as path;
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:tray_manager/tray_manager.dart';
import 'package:uni_platform/uni_platform.dart';
import 'package:win32/win32.dart';
import 'package:window_manager/window_manager.dart';
import '../platform/desktop_platform.dart';
import 'preferences.dart';
import 'async_mutex.dart';

enum QuickPickerMode { management, quick }

class QuickPickerController with TrayListener, WindowListener {
  QuickPickerController._();

  static final instance = QuickPickerController._();
  static const _channel = MethodChannel('sticker_manager/platform');
  static const quickPickerSize = ui.Size(760, 600);
  static const managementDefaultSize = ui.Size(1100, 760);
  static const managementMinimumSize = ui.Size(720, 520);
  static const _windowWidthKey = 'managementWindowWidth';
  static const _windowHeightKey = 'managementWindowHeight';
  static const _windowMaximizedKey = 'managementWindowMaximized';
  static final _defaultHotKey = HotKey(
    key: PhysicalKeyboardKey.keyE,
    modifiers: [
      Platform.isMacOS ? HotKeyModifier.meta : HotKeyModifier.control,
      HotKeyModifier.shift,
    ],
    scope: HotKeyScope.system,
  );
  HWND? _previousWindow;
  int? _previousWindowGeneration;
  int? _applicationWindow;
  final _preferences = AppPreferences();
  HotKey? _hotKey;
  bool _initialized = false;
  bool _exitRequested = false;
  bool _hotKeyRegistered = false;
  bool _hotKeyRegistrationUnavailable = false;
  QuickPickerMode _mode = QuickPickerMode.management;
  Timer? _windowStateSaveTimer;
  DateTime? _lastTrayIconClickAt;
  bool _trayOpenInProgress = false;
  bool _hotKeyOpenInProgress = false;
  int _targetCaptureGeneration = 0;
  final _modeTransitionLock = AsyncMutex();
  void Function()? onPickerShown;
  void Function(QuickPickerMode mode)? onModeChanged;

  HotKey get hotKey => _hotKey ?? _defaultHotKey;

  QuickPickerMode get mode => _mode;

  bool get isQuickPickerMode => _mode == QuickPickerMode.quick;

  Future<void> initialize() async {
    if (!isDesktopPlatform || _initialized) return;
    _initialized = true;
    try {
      _hotKey = await _preferences.hotKeyConfig() ?? _defaultHotKey;
      await windowManager.ensureInitialized();
      windowManager.addListener(this);
      await _restoreManagementWindowState();
      if (Platform.isMacOS) await windowManager.setPreventClose(true);
      try {
        _applicationWindow =
            await _channel.invokeMethod<int>('getWindowHandle');
      } on MissingPluginException {
        _applicationWindow = null;
      }
      _channel.setMethodCallHandler((call) async {
        if (call.method == 'restoreManagementMode') {
          await enterManagementMode(force: true);
          await windowManager.show();
          await windowManager.focus();
          onPickerShown?.call();
        }
        return null;
      });
      if (Platform.isMacOS) {
        // AppKit decodes the same ICO asset used by Windows.
        await trayManager.setIcon('windows/runner/resources/app_icon.ico');
      } else {
        final iconData =
            await rootBundle.load('windows/runner/resources/app_icon.ico');
        final temporary = await getTemporaryDirectory();
        final icon = File(path.join(temporary.path, 'sticker-manager-tray.ico'));
        await icon.writeAsBytes(iconData.buffer.asUint8List(), flush: true);
        await trayManager.setIcon(icon.path);
      }
      await trayManager.setToolTip('表情管家');
      await trayManager.setContextMenu(Menu(items: [
        MenuItem(key: 'show_window', label: '打开表情管家'),
        MenuItem.separator(),
        MenuItem(key: 'exit_app', label: '退出'),
      ]));
      trayManager.addListener(this);
      if (await _isHotKeyAvailable(hotKey)) {
        await hotKeyManager.register(hotKey, keyDownHandler: _handleHotKey);
        _hotKeyRegistered = true;
        _hotKeyRegistrationUnavailable = false;
      } else {
        _hotKeyRegistrationUnavailable = true;
      }
    } on Object {
      final hotKey = _hotKey;
      if (hotKey != null && _hotKeyRegistered) {
        try {
          await hotKeyManager.unregister(hotKey);
        } on Object {
          // The key may not have been registered yet.
        }
      }
      _hotKeyRegistered = false;
      _channel.setMethodCallHandler(null);
      trayManager.removeListener(this);
      windowManager.removeListener(this);
      _windowStateSaveTimer?.cancel();
      try {
        await trayManager.destroy();
      } on Object {
        // Best-effort cleanup must not hide the initialization failure.
      }
      _hotKey = null;
      _applicationWindow = null;
      _initialized = false;
      rethrow;
    }
  }

  Future<void> _handleHotKey(HotKey _) async {
    if (_hotKeyOpenInProgress) return;
    _hotKeyOpenInProgress = true;
    try {
      if (_mode == QuickPickerMode.quick) {
        try {
          if (await windowManager.isVisible()) return;
        } on Object {
          // A stale mode must not make the hotkey unusable when the native
          // visibility query is unavailable during startup or shutdown.
        }
      }
      // Reset the native session before taking the next snapshot. A previous
      // quick-picker invocation can have been hidden before its asynchronous
      // mode callback reached the runner; leaving the flag set would make the
      // runner treat this fresh hotkey as a repeated event.
      await _setNativeQuickPickerActive(false);
      await _captureExternalWindow(fromHotKey: true, consumePending: true);
      await enterQuickPickerMode();
      await windowManager.show();
      await windowManager.focus();
      onPickerShown?.call();
    } finally {
      _hotKeyOpenInProgress = false;
    }
  }

  /// Switches the shared window to the compact layout used by the global
  /// hotkey. The management window's last size and maximized state are kept
  /// separately so opening the picker never loses the user's layout.
  Future<void> enterQuickPickerMode() {
    return _modeTransitionLock.protect(_enterQuickPickerMode);
  }

  Future<void> _enterQuickPickerMode() async {
    if (!isDesktopPlatform) {
      // Notify the UI even when the shared window is already in quick mode.
      // The window can be hidden while its mode remains quick; in that case
      // the next invocation still needs to clear any transient management
      // state (for example, a stale multi-selection).
      _setMode(QuickPickerMode.quick, notifyWhenUnchanged: true);
      return;
    }
    if (_mode == QuickPickerMode.quick) {
      onModeChanged?.call(QuickPickerMode.quick);
      return;
    }
    await _rememberManagementWindowState();
    try {
      if (await windowManager.isMaximized()) {
        await windowManager.unmaximize();
      }
      await windowManager.setSize(quickPickerSize);
      await windowManager.center();
    } on Object {
      // A desktop preview can omit window_manager's native channel. Keep the
      // mode transition usable even when the optional resize is unavailable.
    }
    _setMode(QuickPickerMode.quick);
  }

  /// Restores the management window layout used by the tray entry point.
  Future<void> enterManagementMode({bool force = false}) {
    return _modeTransitionLock
        .protect(() => _enterManagementMode(force: force));
  }

  Future<void> _enterManagementMode({required bool force}) async {
    if (!isDesktopPlatform) {
      _setMode(QuickPickerMode.management);
      return;
    }
    if (_mode == QuickPickerMode.management && !force) return;
    try {
      if (await windowManager.isMaximized()) {
        await windowManager.unmaximize();
      }
      final preferences = await SharedPreferences.getInstance();
      final width = _validWindowDimension(
        preferences.getDouble(_windowWidthKey),
        managementDefaultSize.width,
      );
      final height = _validWindowDimension(
        preferences.getDouble(_windowHeightKey),
        managementDefaultSize.height,
      );
      await windowManager.setSize(ui.Size(width, height));
      if (preferences.getBool(_windowMaximizedKey) ?? false) {
        await windowManager.maximize();
      }
    } on Object {
      // Keep the mode callback useful if the native window has already gone
      // away during shutdown or a desktop test does not load the plugin.
    }
    _setMode(QuickPickerMode.management);
  }

  /// Captures the external top-level window before this app takes focus.
  ///
  /// The native runner normalizes child controls to their GA_ROOT owner. This
  /// matters for QQNT, where the focused editor can be a short-lived child
  /// window that cannot be activated after the picker closes.
  Future<void> _captureExternalWindow({
    bool fromHotKey = false,
    bool? allowRecentManual,
    bool consumePending = false,
  }) async {
    if (!Platform.isWindows) return;
    final generation = ++_targetCaptureGeneration;
    final allowRecent = allowRecentManual ?? !fromHotKey;
    try {
      final raw =
          await _channel.invokeMethod<Object?>('captureExternalWindow', {
        'fromHotKey': fromHotKey,
        'consumePending': consumePending || fromHotKey,
        // Explicit non-hotkey entry points (toolbar or tray) may happen after
        // this window has become foreground. Allow the native runner to reuse
        // only its fresh, short-lived external snapshot. Hotkey calls consume
        // the WM_HOTKEY snapshot instead.
        'allowRecentManual': allowRecent,
      });
      // New runners return the HWND together with a native capture token.
      // Accept the old integer response as a compatibility fallback for a
      // briefly mixed Debug/Release installation.
      int? rawHandle;
      int? captureGeneration;
      if (raw is int) {
        rawHandle = raw;
      } else if (raw is Map) {
        final handleValue = raw['handle'];
        final generationValue = raw['generation'];
        if (handleValue is int) rawHandle = handleValue;
        if (generationValue is int) captureGeneration = generationValue;
      }
      final capturedWindow = rawHandle == null || rawHandle == 0
          ? null
          : HWND(Pointer.fromAddress(rawHandle));
      if (generation != _targetCaptureGeneration) {
        // A newer capture may already have frozen the same HWND. Do not issue
        // an HWND-only cleanup here: an old asynchronous result could erase
        // the newer target when the window was reused.
        return;
      }
      _previousWindow = capturedWindow;
      _previousWindowGeneration = captureGeneration;
      return;
    } on MissingPluginException {
      // Desktop previews may not include the custom runner channel.
    } on PlatformException {
      // Fall back to the Dart Win32 binding below.
    }

    if (generation != _targetCaptureGeneration) return;
    final foreground = GetForegroundWindow();
    if (foreground.address == 0) {
      if (generation != _targetCaptureGeneration) return;
      _previousWindow = null;
      _previousWindowGeneration = null;
      return;
    }
    final applicationWindow = _applicationWindow;
    var isApplicationWindow =
        applicationWindow != null && foreground.address == applicationWindow;
    if (!isApplicationWindow) {
      try {
        isApplicationWindow = await _channel.invokeMethod<bool>(
                'isApplicationWindow', foreground.address) ??
            false;
      } on MissingPluginException {
        // Handle comparison remains the safe fallback.
      } on PlatformException {
        // Treat an unknown window as external in the preview fallback.
      }
    }
    if (generation != _targetCaptureGeneration) return;
    final fallbackWindow = isApplicationWindow ? null : foreground;
    _previousWindow = fallbackWindow;
    _previousWindowGeneration = null;
  }

  Future<void> captureExternalWindow() =>
      _captureExternalWindow(fromHotKey: false, allowRecentManual: true);

  Future<bool> updateHotKey(HotKey next) async {
    if (!isDesktopPlatform || !_isUsableHotKey(next)) return false;
    final previous = _hotKey;
    final previousRegistered = _hotKeyRegistered;
    if (previous?.debugName == next.debugName) return true;
    if (!await _isHotKeyAvailable(next)) {
      _hotKeyRegistrationUnavailable = !previousRegistered;
      return false;
    }
    var nextRegistered = false;
    try {
      if (previous != null && previousRegistered) {
        await hotKeyManager.unregister(previous);
        _hotKeyRegistered = false;
      }
      await hotKeyManager.register(next, keyDownHandler: _handleHotKey);
      nextRegistered = true;
      _hotKey = next;
      _hotKeyRegistered = true;
      _hotKeyRegistrationUnavailable = false;
      await _preferences.setHotKeyConfig(next);
      return true;
    } on Object {
      try {
        if (nextRegistered) {
          await hotKeyManager.unregister(next);
          _hotKeyRegistered = false;
        }
        if (previous != null && previousRegistered) {
          await hotKeyManager.register(previous, keyDownHandler: _handleHotKey);
          _hotKeyRegistered = true;
        }
      } on Object {
        // Keep the failed registration from masking the original error.
      }
      _hotKey = previous;
      _hotKeyRegistrationUnavailable = !_hotKeyRegistered;
      return false;
    }
  }

  bool get hotKeyRegistrationUnavailable => _hotKeyRegistrationUnavailable;

  Future<bool> _isHotKeyAvailable(HotKey value) async {
    if (!isDesktopPlatform) return true;
    try {
      return await _channel.invokeMethod<bool>('isHotKeyAvailable', {
            'keyCode': value.physicalKey.keyCode,
            'modifiers':
                value.modifiers?.map((modifier) => modifier.name).toList() ??
                    const <String>[],
          }) ??
          false;
    } on MissingPluginException {
      // Keep desktop previews usable when the custom runner channel is absent.
      return true;
    } on PlatformException {
      return false;
    }
  }

  bool _isUsableHotKey(HotKey value) {
    final physical = value.physicalKey;
    final isModifier = HotKeyModifier.values
        .any((modifier) => modifier.physicalKeys.contains(physical));
    return !isModifier && (value.modifiers?.isNotEmpty ?? false);
  }

  bool get hasPreviousWindow {
    final handle = _previousWindow;
    return Platform.isWindows &&
        handle != null &&
        handle.address != 0 &&
        IsWindow(handle);
  }

  /// Ensures that a Windows send target exists immediately before a card is
  /// used. The management window can stay open while the user switches to QQ
  /// or another chat client; refreshing in that mode binds the click to the
  /// most recent external foreground window instead of the open-time target.
  /// Quick-picker mode keeps the one-shot target captured by the hotkey.
  Future<bool> ensurePreviousWindow({bool refresh = false}) async {
    if (!Platform.isWindows) return false;
    if (!refresh && hasPreviousWindow) return true;
    await _captureExternalWindow(
      fromHotKey: false,
      allowRecentManual: true,
      consumePending: false,
    );
    return hasPreviousWindow;
  }

  Future<String?> previousWindowProcessName() async {
    final generation = _targetCaptureGeneration;
    final handle = _previousWindow;
    if (!Platform.isWindows || handle == null || handle.address == 0) {
      return null;
    }
    if (!IsWindow(handle)) {
      if (generation == _targetCaptureGeneration) discardPreviousWindow();
      return null;
    }
    final current = await _readWindowProcessName(handle);
    if (generation != _targetCaptureGeneration) return null;
    // A temporary native process-query failure must not invalidate a still
    // valid HWND. Activation performs its own identity checks and can proceed
    // even when the display name is unavailable.
    return current;
  }

  Future<String?> _readWindowProcessName(HWND handle) async {
    if (!IsWindow(handle)) return null;
    try {
      final name = await _channel.invokeMethod<String>(
        'getWindowProcessName',
        handle.address,
      );
      final normalized = name?.trim();
      return normalized == null || normalized.isEmpty ? null : normalized;
    } on MissingPluginException {
      return null;
    } on PlatformException {
      return null;
    }
  }

  Future<bool> activatePreviousWindow() async {
    final handle = _previousWindow;
    ++_targetCaptureGeneration;
    _previousWindow = null;
    _previousWindowGeneration = null;
    if (!Platform.isWindows || handle == null || handle.address == 0) {
      await enterManagementMode(force: true);
      return false;
    }
    if (IsWindow(handle) && IsIconic(handle)) ShowWindow(handle, SW_RESTORE);
    // Sending from the quick picker ends that mode. Restore the management
    // layout before hiding so window-manager state changes cannot race with
    // target activation or overwrite the captured foreground context.
    await enterManagementMode(force: true);
    await windowManager.hide();
    await Future<void>.delayed(const Duration(milliseconds: 80));
    try {
      final activated = await _channel.invokeMethod<bool>(
            'activateWindow',
            handle.address,
          ) ??
          false;
      if (activated) return true;
    } on MissingPluginException {
      // Fall back to the Dart Win32 binding in desktop previews.
    } on PlatformException {
      // The direct Win32 call below is still useful when the runner channel
      // cannot attach to the target thread.
    }
    if (!IsWindow(handle)) return false;
    for (var attempt = 0; attempt < 6; attempt++) {
      SetForegroundWindow(handle);
      if (_isForegroundTopLevel(handle)) return true;
      await Future<void>.delayed(const Duration(milliseconds: 20));
    }
    return false;
  }

  bool _isForegroundTopLevel(HWND target) {
    if (!IsWindow(target)) return false;
    final foreground = GetForegroundWindow();
    if (foreground.address == 0) return false;
    final targetRoot = GetAncestor(target, GA_ROOT);
    final foregroundRoot = GetAncestor(foreground, GA_ROOT);
    return targetRoot.address != 0 &&
        targetRoot.address == foregroundRoot.address;
  }

  void discardPreviousWindow() {
    ++_targetCaptureGeneration;
    final expectedHandle = _previousWindow?.address ?? 0;
    final expectedGeneration = _previousWindowGeneration;
    _previousWindow = null;
    _previousWindowGeneration = null;
    if (Platform.isWindows && expectedHandle != 0) {
      unawaited(_clearNativeTarget(expectedHandle, expectedGeneration));
    }
  }

  Future<void> _clearNativeTarget(
      int expectedHandle, int? expectedGeneration) async {
    try {
      await _channel.invokeMethod<void>('clearExternalWindowTarget', {
        'expectedHandle': expectedHandle,
        if (expectedGeneration != null && expectedGeneration != 0)
          'expectedGeneration': expectedGeneration,
      });
    } on MissingPluginException {
      // Desktop previews may not include the custom runner channel.
    } on PlatformException {
      // The Dart-side one-shot handle is already cleared.
    }
  }

  void notifyPickerShown() {
    onPickerShown?.call();
  }

  Future<void> dispose() async {
    if (!isDesktopPlatform || !_initialized) return;
    ++_targetCaptureGeneration;
    await _rememberManagementWindowState();
    _initialized = false;
    trayManager.removeListener(this);
    windowManager.removeListener(this);
    _windowStateSaveTimer?.cancel();
    _lastTrayIconClickAt = null;
    _trayOpenInProgress = false;
    final hotKey = _hotKey;
    if (hotKey != null && _hotKeyRegistered) {
      await hotKeyManager.unregister(hotKey);
    }
    _hotKeyRegistered = false;
    _channel.setMethodCallHandler(null);
    await trayManager.destroy();
  }

  @override
  void onTrayIconMouseDown() {
    // tray_manager reports both clicks of a Windows double-click as separate
    // mouse-up events. Coalesce them so the second event cannot clear the
    // target captured by the first one.
    if (_trayOpenInProgress) return;
    final now = DateTime.now();
    final previous = _lastTrayIconClickAt;
    if (previous != null &&
        now.difference(previous) < const Duration(milliseconds: 800)) {
      return;
    }
    _lastTrayIconClickAt = now;
    _trayOpenInProgress = true;
    unawaited(_showFromTray(captureTarget: true));
  }

  @override
  void onTrayIconRightMouseDown() {
    // tray_manager deliberately leaves context-menu display to the app on
    // Windows. Without this explicit call a right click appears to do nothing.
    unawaited(_showTrayContextMenu());
  }

  @override
  void onTrayMenuItemClick(MenuItem menuItem) {
    if (menuItem.key == 'show_window') {
      // The context-menu command is an explicit management action. It must
      // not inherit the target captured by a tray-icon click.
      unawaited(_showFromTray(captureTarget: false));
    } else if (menuItem.key == 'exit_app') {
      unawaited(_exitFromTray());
    }
  }

  Future<void> _exitFromTray() async {
    if (_exitRequested) return;
    _exitRequested = true;
    try {
      await dispose();
    } finally {
      // The native runner owns the process mutex. Always terminate after the
      // best-effort plugin cleanup so the mutex cannot be held by a stale UI.
      exit(0);
    }
  }

  Future<void> _showFromTray({required bool captureTarget}) async {
    try {
      if (captureTarget) {
        // Capture while the external application is still foreground. The
        // native runner may fall back to its short-lived external snapshot if
        // the shell has already activated this window.
        await _captureExternalWindow(
            fromHotKey: false, allowRecentManual: true, consumePending: true);
      } else {
        discardPreviousWindow();
      }
      await enterManagementMode(force: true);
      await windowManager.show();
      await windowManager.focus();
      onPickerShown?.call();
    } finally {
      if (captureTarget) _trayOpenInProgress = false;
    }
  }

  @override
  void onWindowClose() {
    // Closing/hiding the full window ends the one-shot send context. A later
    // tray open will capture a fresh foreground target.
    discardPreviousWindow();
    if (Platform.isMacOS) unawaited(windowManager.hide());
  }

  Future<void> _showTrayContextMenu() async {
    try {
      await trayManager.popUpContextMenu();
    } on Object {
      // The tray may be destroyed concurrently during process shutdown.
    }
  }

  void _setMode(QuickPickerMode next, {bool notifyWhenUnchanged = false}) {
    if (_mode == next) {
      if (notifyWhenUnchanged) onModeChanged?.call(next);
      if (Platform.isWindows) {
        unawaited(_setNativeQuickPickerActive(next == QuickPickerMode.quick));
      }
      return;
    }
    _mode = next;
    onModeChanged?.call(next);
    if (Platform.isWindows) {
      unawaited(_setNativeQuickPickerActive(next == QuickPickerMode.quick));
    }
  }

  Future<void> _setNativeQuickPickerActive(bool active) async {
    if (!Platform.isWindows) return;
    try {
      await _channel
          .invokeMethod<void>('setQuickPickerActive', {'active': active});
    } on MissingPluginException {
      // Desktop previews may not include the custom runner channel.
    } on PlatformException {
      // The Dart mode remains authoritative when an older runner is used.
    }
  }

  double _validWindowDimension(double? value, double fallback) {
    if (value == null || !value.isFinite || value < 320 || value > 10000) {
      return fallback;
    }
    return value;
  }

  Future<void> _restoreManagementWindowState() async {
    if (!isDesktopPlatform) return;
    try {
      final preferences = await SharedPreferences.getInstance();
      final width = _validWindowDimension(
        preferences.getDouble(_windowWidthKey),
        managementDefaultSize.width,
      );
      final height = _validWindowDimension(
        preferences.getDouble(_windowHeightKey),
        managementDefaultSize.height,
      );
      await windowManager.setMinimumSize(managementMinimumSize);
      await windowManager.setSize(ui.Size(width, height));
      if (preferences.getBool(_windowMaximizedKey) ?? false) {
        await windowManager.maximize();
      }
    } on Object {
      // Native window sizing is an enhancement; startup must remain usable if
      // the plugin is unavailable in a preview or test environment.
    }
  }

  Future<void> _rememberManagementWindowState() async {
    if (!isDesktopPlatform || _mode != QuickPickerMode.management) return;
    try {
      final maximized = await windowManager.isMaximized();
      final preferences = await SharedPreferences.getInstance();
      await preferences.setBool(_windowMaximizedKey, maximized);
      if (maximized) return;
      final size = await windowManager.getSize();
      if (!_isUsableWindowSize(size)) return;
      await preferences.setDouble(_windowWidthKey, size.width);
      await preferences.setDouble(_windowHeightKey, size.height);
    } on Object {
      // Best effort only; a size write must never prevent tray shutdown.
    }
  }

  bool _isUsableWindowSize(ui.Size size) {
    return size.width.isFinite &&
        size.height.isFinite &&
        size.width >= managementMinimumSize.width &&
        size.height >= managementMinimumSize.height &&
        size.width <= 10000 &&
        size.height <= 10000;
  }

  void _scheduleManagementWindowStateSave() {
    if (!isDesktopPlatform || _mode != QuickPickerMode.management) return;
    _windowStateSaveTimer?.cancel();
    _windowStateSaveTimer = Timer(const Duration(milliseconds: 350), () {
      unawaited(_rememberManagementWindowState());
    });
  }

  @override
  void onWindowResized() {
    _scheduleManagementWindowStateSave();
  }

  @override
  void onWindowUnmaximize() {
    _scheduleManagementWindowStateSave();
  }

  @override
  void onWindowMaximize() {
    if (!isDesktopPlatform || _mode != QuickPickerMode.management) return;
    unawaited(SharedPreferences.getInstance().then(
      (preferences) => preferences.setBool(_windowMaximizedKey, true),
    ));
  }
}
