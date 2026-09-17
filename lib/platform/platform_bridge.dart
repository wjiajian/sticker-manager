import 'dart:io';

import 'package:flutter/services.dart';
import 'package:window_manager/window_manager.dart';

import '../models.dart';
import '../services/async_mutex.dart';
import '../services/quick_picker_controller.dart';
import 'clipboard_bridge.dart';
import 'desktop_platform.dart';

enum StickerUseStatus { copied, sent, failed }

class StickerUseResult {
  const StickerUseResult(this.status,
      {this.message, this.targetApplication, this.didPaste = false});

  const StickerUseResult.copied(
      {String? message, String? targetApplication, bool didPaste = false})
      : this(StickerUseStatus.copied,
            message: message,
            targetApplication: targetApplication,
            didPaste: didPaste);
  const StickerUseResult.sent({String? targetApplication})
      : this(StickerUseStatus.sent,
            targetApplication: targetApplication, didPaste: true);
  const StickerUseResult.failed(String message, {String? targetApplication})
      : this(StickerUseStatus.failed,
            message: message, targetApplication: targetApplication);

  final StickerUseStatus status;
  final String? message;
  final String? targetApplication;

  /// True only after the media was actually handed to the target input.
  /// Copying into the system clipboard by itself is not a successful paste.
  final bool didPaste;

  bool get succeeded => status != StickerUseStatus.failed;
}

class PlatformBridge {
  PlatformBridge._();

  static final instance = PlatformBridge._();
  static const _channel = MethodChannel('sticker_manager/platform');
  static const _shareEvents = EventChannel('sticker_manager/share_events');
  final _windowsUseLock = AsyncMutex();

  Stream<List<String>> get sharedFiles {
    return _shareEvents.receiveBroadcastStream().map((event) {
      if (event is! List) return const <String>[];
      return event.whereType<String>().toList(growable: false);
    });
  }

  Future<void> initialize() async {
    if (isDesktopPlatform) {
      await windowManager.ensureInitialized();
      return;
    }
    if (!Platform.isAndroid) return;
    try {
      await _channel.invokeMethod<void>('initialize');
    } on MissingPluginException {
      // Keep the UI usable in a desktop preview or a partially registered build.
    }
  }

  Future<StickerUseResult> useSticker(Sticker sticker) async {
    if (Platform.isWindows) return _useWindows(sticker);
    return _copyOnly(sticker);
  }

  Future<StickerUseResult> _copyOnly(Sticker sticker) async {
    if (Platform.isMacOS) {
      final copied = await ClipboardBridge.instance.writeSticker(sticker);
      return copied
          ? const StickerUseResult.copied(message: '已复制，请切换到目标应用按 ⌘V 粘贴')
          : const StickerUseResult.failed('无法写入 macOS 剪贴板');
    }
    if (!Platform.isAndroid) {
      return const StickerUseResult.failed('当前平台不支持系统剪贴板');
    }
    try {
      final copied = await _channel.invokeMethod<bool>('pasteSticker', {
            'path': sticker.filePath,
            'mediaType': enumValue(sticker.mediaType),
          }) ??
          false;
      return copied
          ? const StickerUseResult.copied()
          : const StickerUseResult.failed('无法写入 Android 剪贴板');
    } on MissingPluginException {
      return const StickerUseResult.failed('当前 Android 构建未接入剪贴板');
    } on PlatformException catch (error) {
      return StickerUseResult.failed(error.message ?? '无法写入 Android 剪贴板');
    }
  }

  /// Copies a sticker to the system clipboard without activating a target
  /// window, pasting or sending Enter. Windows shares the use-lock so an
  /// independent copy cannot interleave with an in-flight send.
  Future<StickerUseResult> copySticker(Sticker sticker) async {
    if (Platform.isWindows) {
      return _windowsUseLock.protect(() async {
        final copied = await ClipboardBridge.instance.writeSticker(sticker);
        return copied
            ? const StickerUseResult.copied()
            : const StickerUseResult.failed('无法写入 Windows 剪贴板');
      });
    }
    return _copyOnly(sticker);
  }

  Future<void> showQuickPicker() async {
    if (!isDesktopPlatform) return;
    final picker = QuickPickerController.instance;
    await picker.captureExternalWindow();
    await picker.enterQuickPickerMode();
    await windowManager.show();
    await windowManager.focus();
    picker.notifyPickerShown();
  }

  Future<List<String>> consumeSharedFiles() async {
    if (!Platform.isAndroid) return const [];
    try {
      final result =
          await _channel.invokeMethod<List<Object?>>('consumeSharedFiles');
      return result?.whereType<String>().toList() ?? const [];
    } on MissingPluginException {
      return const [];
    }
  }

  /// Returns share-copy failures recorded by the Android activity. The
  /// activity persists these while Flutter is not running, so an oversized or
  /// unreadable URI is reported instead of disappearing silently.
  Future<List<String>> consumeShareErrors() async {
    if (!Platform.isAndroid) return const [];
    try {
      final result =
          await _channel.invokeMethod<List<Object?>>('consumeShareErrors');
      return result?.whereType<String>().toList() ?? const [];
    } on MissingPluginException {
      return const [];
    } on PlatformException {
      return const [];
    }
  }

  Future<void> acknowledgeSharedFiles(Iterable<String> files) async {
    if (!Platform.isAndroid) return;
    final paths = files.toList(growable: false);
    if (paths.isEmpty) return;
    try {
      await _channel.invokeMethod<void>('ackSharedFiles', {'files': paths});
    } on MissingPluginException {
      // Desktop previews do not expose the Android share queue channel.
    }
  }

  Future<bool> isOverlayGranted() async {
    if (!Platform.isAndroid) return false;
    try {
      return await _channel.invokeMethod<bool>('isOverlayGranted') ?? false;
    } on MissingPluginException {
      return false;
    }
  }

  Future<bool> isFloatingPanelRunning() async {
    if (!Platform.isAndroid) return false;
    try {
      return await _channel.invokeMethod<bool>('isFloatingPanelRunning') ??
          false;
    } on MissingPluginException {
      return false;
    }
  }

  Future<List<String>> peekFloatingUsage() async {
    if (!Platform.isAndroid) return const [];
    try {
      final result =
          await _channel.invokeMethod<List<Object?>>('peekFloatingUsage');
      return result?.whereType<String>().toList(growable: false) ?? const [];
    } on MissingPluginException {
      return const [];
    }
  }

  Future<void> acknowledgeFloatingUsage(Iterable<String> ids) async {
    if (!Platform.isAndroid) return;
    final values = ids.toList(growable: false);
    if (values.isEmpty) return;
    try {
      await _channel.invokeMethod<void>('ackFloatingUsage', {'ids': values});
    } on MissingPluginException {
      // Desktop previews do not expose the Android overlay queue channel.
    }
  }

  Future<bool> startFloatingPanel(Iterable<Sticker> stickers) async {
    if (!Platform.isAndroid) return false;
    try {
      if (!await isOverlayGranted()) {
        await openOverlaySettings();
        return false;
      }
      final payload = stickers
          .take(100)
          .map((sticker) => {
                'id': sticker.id,
                'path': sticker.filePath,
                'mediaType': enumValue(sticker.mediaType),
                'note': sticker.note,
              })
          .toList(growable: false);
      return await _channel.invokeMethod<bool>('startFloatingPanel', {
            'stickers': payload,
          }) ??
          false;
    } on MissingPluginException {
      return false;
    } on PlatformException {
      return false;
    }
  }

  Future<void> stopFloatingPanel() async {
    if (!Platform.isAndroid) return;
    try {
      await _channel.invokeMethod<void>('stopFloatingPanel');
    } on MissingPluginException {
      // Keep the library usable when running without the Android runner.
    }
  }

  Future<void> syncFloatingPanel(Iterable<Sticker> stickers) async {
    if (!Platform.isAndroid) return;
    try {
      final payload = stickers
          .take(100)
          .map((sticker) => {
                'id': sticker.id,
                'path': sticker.filePath,
                'mediaType': enumValue(sticker.mediaType),
                'note': sticker.note,
              })
          .toList(growable: false);
      await _channel.invokeMethod<void>('syncFloatingPanel', {
        'stickers': payload,
      });
    } on MissingPluginException {
      // Desktop previews do not expose the Android overlay channel.
    }
  }

  Future<void> openOverlaySettings() async {
    if (!Platform.isAndroid) return;
    try {
      await _channel.invokeMethod<void>('openOverlaySettings');
    } on MissingPluginException {
      // No system settings page is available outside Android.
    }
  }

  Future<StickerUseResult> _useWindows(Sticker sticker) {
    // Clipboard contents are process-global. Serialize the complete Windows
    // use flow so a second card click cannot replace the first sticker between
    // copy and the injected Ctrl+V.
    return _windowsUseLock.protect(() => _useWindowsLocked(sticker));
  }

  Future<StickerUseResult> _useWindowsLocked(Sticker sticker) async {
    final picker = QuickPickerController.instance;
    // The management window may have been opened before the user switched to
    // QQ/WeChat. Recover the most recent valid external window at the moment
    // of use instead of treating the open-time snapshot as permanent.
    // A full management window may remain open while the user switches to a
    // different chat client. Refresh its target at click time; the compact
    // hotkey picker keeps the one-shot target captured before it appeared.
    await picker.ensurePreviousWindow(refresh: !picker.isQuickPickerMode);
    final targetApplication = await picker.previousWindowProcessName();
    final copied = await ClipboardBridge.instance.writeSticker(sticker);
    if (!copied) {
      picker.discardPreviousWindow();
      return StickerUseResult.failed('无法写入 Windows 剪贴板',
          targetApplication: targetApplication);
    }

    if (!picker.hasPreviousWindow) {
      picker.discardPreviousWindow();
      return StickerUseResult.copied(targetApplication: targetApplication);
    }
    if (!await picker.activatePreviousWindow()) {
      return StickerUseResult.failed('发送目标窗口已关闭或无法激活',
          targetApplication: targetApplication);
    }
    // Chromium-based clients can finish activating their editor a little
    // after SetForegroundWindow returns. Give the target queue one frame
    // before injecting Ctrl+V.
    await Future<void>.delayed(const Duration(milliseconds: 80));
    if (!await ClipboardBridge.instance.sendPaste()) {
      picker.discardPreviousWindow();
      return StickerUseResult.failed('无法向目标窗口发送粘贴操作',
          targetApplication: targetApplication);
    }
    // QQ/QQNT may decode a large image or import a GIF file asynchronously.
    // Sending Enter immediately after Ctrl+V is therefore lossy: the paste is
    // visible but the Enter event is consumed while the editor is busy.
    await Future<void>.delayed(const Duration(milliseconds: 500));
    if (!await ClipboardBridge.instance.sendEnter()) {
      picker.discardPreviousWindow();
      // The paste completed even though the optional send action failed, so
      // callers can still count this use without reporting a false failure.
      return StickerUseResult.copied(
          message: '已粘贴，但无法发送回车',
          targetApplication: targetApplication,
          didPaste: true);
    }
    picker.discardPreviousWindow();
    return StickerUseResult.sent(targetApplication: targetApplication);
  }
}
