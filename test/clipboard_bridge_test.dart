import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sticker_manager/models.dart';
import 'package:sticker_manager/platform/clipboard_bridge.dart';
import 'package:sticker_manager/platform/platform_bridge.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('sticker_manager/platform');
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  late Directory temporary;

  setUp(() async {
    temporary = await Directory.systemTemp.createTemp('sticker-clipboard-');
  });
  tearDown(() async {
    messenger.setMockMethodCallHandler(channel, null);
    await temporary.delete(recursive: true);
  });

  Sticker sticker(File file, StickerMediaType type) => Sticker(
        id: 'test',
        hash: 'test',
        mediaType: type,
        filePath: file.path,
        thumbnailPath: '',
        source: StickerSource.manual,
        createdAt: DateTime(2026),
        updatedAt: DateTime(2026),
      );

  test('missing media does not invoke the native clipboard', () async {
    messenger.setMockMethodCallHandler(channel, (_) async {
      fail('Missing files must not alter the clipboard');
    });
    final value =
        sticker(File('${temporary.path}/missing.png'), StickerMediaType.image);
    expect(await ClipboardBridge.instance.writeSticker(value), isFalse);
  });

  test('native clipboard receives the original GIF path and media type',
      () async {
    final file =
        await File('${temporary.path}/animation.gif').writeAsBytes([1]);
    final calls = <MethodCall>[];
    messenger.setMockMethodCallHandler(channel, (call) async {
      calls.add(call);
      return true;
    });
    expect(
        await ClipboardBridge.instance
            .writeSticker(sticker(file, StickerMediaType.gif)),
        isTrue);
    expect(calls.single.method, 'copySticker');
    expect(calls.single.arguments, {'path': file.path, 'mediaType': 'gif'});
  });

  test('macOS copies without injecting paste or Enter', () async {
    final file = await File('${temporary.path}/image.png').writeAsBytes([1]);
    final calls = <String>[];
    messenger.setMockMethodCallHandler(channel, (call) async {
      calls.add(call.method);
      return true;
    });
    final result = await PlatformBridge.instance
        .useSticker(sticker(file, StickerMediaType.image));
    expect(result.status, StickerUseStatus.copied);
    expect(result.didPaste, isFalse);
    expect(result.message, contains('⌘V'));
    expect(calls, ['copySticker']);
  }, skip: !Platform.isMacOS);
}
