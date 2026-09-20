import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/services.dart';
import 'package:pasteboard/pasteboard.dart';
import 'package:path/path.dart' as path;

import '../models.dart';
import '../services/media_store.dart';

class ClipboardBridge {
  ClipboardBridge._();

  static final instance = ClipboardBridge._();
  static const _channel = MethodChannel('sticker_manager/platform');

  Future<ClipboardImport> readForImport() async {
    final paths = await Pasteboard.files();
    if (paths.isNotEmpty) {
      return ClipboardImport(paths.map((value) {
        final uri = Uri.tryParse(value);
        return File(uri?.scheme == 'file' ? uri!.toFilePath() : value);
      }).toList());
    }
    Uint8List? bytes;
    try {
      bytes = await _channel.invokeMethod<Uint8List>('readClipboardImageData');
    } on MissingPluginException {
      // Preview runners can still import a static bitmap via the plugin.
    }
    bytes ??= await Pasteboard.image;
    if (bytes == null || bytes.isEmpty) return const ClipboardImport([]);
    if (bytes.length > MediaStore.maxImportFileBytes) {
      throw const FormatException('剪贴板图片超过 64 MiB');
    }
    final directory =
        await Directory.systemTemp.createTemp('sticker-clipboard-');
    try {
      final gif =
          bytes.length >= 6 && String.fromCharCodes(bytes.take(3)) == 'GIF';
      final stamp = DateTime.now().toIso8601String().replaceAll(':', '-');
      final file = File(
          path.join(directory.path, '剪贴板图片 $stamp.${gif ? 'gif' : 'png'}'));
      await file.writeAsBytes(bytes);
      return ClipboardImport([file], temporaryDirectory: directory);
    } on Object {
      await directory.delete(recursive: true);
      rethrow;
    }
  }

  Future<bool> writeSticker(Sticker sticker) async {
    final file = File(sticker.filePath);
    if (!await file.exists()) return false;
    try {
      final copied = await _channel.invokeMethod<bool>('copySticker', {
            'path': file.path,
            'mediaType': enumValue(sticker.mediaType),
          }) ??
          false;
      if (copied) return true;
      if (sticker.mediaType == StickerMediaType.gif) {
        // The native runner owns the preferred CF_HDROP path, but a busy
        // clipboard can reject it briefly. Keep GIF animation by retrying
        // through the platform file-list writer instead of decoding a frame.
        return await Pasteboard.writeFiles([file.path]);
      }
      return await _writeStaticImageAsPng(file);
    } on MissingPluginException {
      // A Flutter desktop preview may not have the runner channel yet.
      try {
        if (sticker.mediaType == StickerMediaType.gif) {
          return await Pasteboard.writeFiles([file.path]);
        }
        await Pasteboard.writeImage(await file.readAsBytes());
        return true;
      } on Object {
        return false;
      }
    } on PlatformException {
      if (sticker.mediaType == StickerMediaType.gif) {
        try {
          return await Pasteboard.writeFiles([file.path]);
        } on Object {
          return false;
        }
      }
      return _writeStaticImageAsPng(file);
    }
  }

  Future<bool> _writeStaticImageAsPng(File file) async {
    ui.Codec? codec;
    ui.FrameInfo? frame;
    try {
      codec = await ui.instantiateImageCodec(await file.readAsBytes());
      frame = await codec.getNextFrame();
      final data = await frame.image.toByteData(format: ui.ImageByteFormat.png);
      if (data == null) return false;
      await Pasteboard.writeImage(data.buffer.asUint8List());
      return true;
    } on Object {
      return false;
    } finally {
      frame?.image.dispose();
      codec?.dispose();
    }
  }

  Future<bool> sendPaste() => _send('sendPaste');

  Future<bool> sendEnter() => _send('sendEnter');

  Future<bool> _send(String method) async {
    try {
      return await _channel.invokeMethod<bool>(method) ?? false;
    } on MissingPluginException {
      return false;
    } on PlatformException {
      return false;
    }
  }
}

class ClipboardImport {
  const ClipboardImport(this.files, {this.temporaryDirectory});
  final List<File> files;
  final Directory? temporaryDirectory;
  Future<void> dispose() async {
    final directory = temporaryDirectory;
    if (directory != null && await directory.exists()) {
      await directory.delete(recursive: true);
    }
  }
}
