import 'dart:io';

/// Platforms with a tray, global hotkey and resizable picker window.
bool get isDesktopPlatform => Platform.isWindows || Platform.isMacOS;
