import 'dart:convert';
import 'dart:io';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:sticker_manager/platform/desktop_drop_bridge.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sticker_manager/models.dart';
import 'package:sticker_manager/services/database.dart';
import 'package:sticker_manager/services/export_service.dart';
import 'package:sticker_manager/services/media_store.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Directory root;
  late StickerDatabase db;
  late MediaStore store;
  late File png;
  setUp(() async {
    root = await Directory.systemTemp.createTemp('sticker-import-workflow-');
    db = StickerDatabase(databasePath: '${root.path}/db.sqlite');
    store = MediaStore(db, mediaDirectory: Directory('${root.path}/media'));
    png = await File('${root.path}/cat.png').writeAsBytes(base64Decode(
        'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aN1cAAAAASUVORK5CYII='));
    await db.createGroup('work', '工作');
    await db.createGroup('fun', '娱乐');
  });
  tearDown(() async {
    await db.close();
    await root.delete(recursive: true);
  });

  test(
      'preview is read-only and reports missing, unsupported and oversized files',
      () async {
    final text =
        await File('${root.path}/text.txt').writeAsString('not an image');
    final large = File('${root.path}/large.png');
    final handle = await large.open(mode: FileMode.write);
    await handle.truncate(MediaStore.maxImportFileBytes + 1);
    await handle.close();
    final preview = await store
        .inspectFiles([png, text, large, File('${root.path}/missing')]);
    expect(preview.map((c) => c.valid), [true, false, false, false]);
    expect(preview[2].error, contains('64 MiB'));
    expect(await db.loadRanked(), isEmpty);
    expect(await Directory('${root.path}/media').exists(), isFalse);
    expect(await png.exists(), isTrue);
  });

  test('duplicate adds membership without copying media or changing metadata',
      () async {
    await store.importFiles([png], groupIds: ['all', 'work']);
    final before = (await db.loadRanked()).single;
    await db
        .updateSticker(before.sticker.copyWith(note: '保留备注', usageCount: 9));
    final managed = File(before.sticker.filePath);
    final originalModified = (await managed.stat()).modified;
    final preview = await store.inspectFiles([png]);
    expect(preview.single.existing!.groupIds, contains('work'));
    final result = await store.importFiles([png], groupIds: ['all', 'fun']);
    expect(result.added, 0);
    expect(result.existingGrouped, 1);
    expect(result.alreadyExists, 0);
    final after = (await db.loadRanked()).single;
    expect(after.sticker.note, '保留备注');
    expect(after.sticker.usageCount, 9);
    expect(after.groupIds, containsAll(['all', 'work', 'fun']));
    expect((await managed.stat()).modified, originalModified);
    final again = await store.importFiles([png], groupIds: ['all', 'fun']);
    expect(again.existingGrouped, 0);
    expect(again.alreadyExists, 1);
  });

  for (final missing in [true, false]) {
    for (final fromPackage in [false, true]) {
      test(
          '${fromPackage ? 'package restore' : 'reimport'} repairs ${missing ? 'missing' : 'truncated'} managed media',
          () async {
        final originalBytes = await png.readAsBytes();
        await store.importFiles([png], groupIds: ['all', 'work']);
        final before = (await db.loadRanked()).single;
        await db.updateSticker(
            before.sticker.copyWith(note: '保留备注', usageCount: 9));
        final exporter = ExportPackageService(db);
        final package = File('${root.path}/backup.smp');
        if (fromPackage) {
          await package
              .writeAsBytes(await exporter.buildPackage('password123'));
        }
        final managed = File(before.sticker.filePath);
        if (missing) {
          await managed.delete();
        } else {
          await managed.writeAsBytes(originalBytes.take(8).toList());
        }

        final preview = (await store.inspectFiles([png])).single;
        expect(preview.existing!.sticker.id, before.sticker.id);
        expect(await managed.exists(), !missing);
        if (!missing) expect(await managed.length(), 8);

        final result = fromPackage
            ? await exporter.importFrom(package, 'password123', store)
            : await store.importFiles([png],
                expectedHashes: {png.path: preview.hash!},
                groupIds: ['all', 'work']);
        expect(result.added, 0);
        expect(result.duplicates, 1);
        expect(result.skipped, 0);
        expect(await managed.readAsBytes(), originalBytes);
        expect(await png.readAsBytes(), originalBytes);
        final after = (await db.loadRanked()).single;
        expect(after.sticker.id, before.sticker.id);
        expect(after.sticker.note, '保留备注');
        expect(after.sticker.usageCount, 9);
        expect(after.groupIds, before.groupIds);
      });
    }
  }

  test('drop data keeps GIF bytes and cleanup leaves original files untouched',
      () async {
    final bytes = base64Decode(
        'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7');
    final input = DropImport([
      DropItemFile(png.path),
      DropItemFile.fromData(bytes, name: 'animation.gif')
    ]);
    await input.read(directorySource: WindowsImportSource());
    expect(await input.files.last.readAsBytes(), bytes);
    final generated = input.files.last;
    await input.dispose();
    expect(await generated.exists(), isFalse);
    expect(await png.exists(), isTrue);
  });

  test('all preview sources share the aggregate 512 MiB budget', () async {
    final files = <File>[];
    for (var i = 0; i < 9; i++) {
      final file = File('${root.path}/sparse-$i.png');
      final handle = await file.open(mode: FileMode.write);
      await handle.truncate(MediaStore.maxImportFileBytes);
      await handle.close();
      files.add(file);
    }
    final preview = await store.inspectFiles(files);
    expect(preview.last.error, contains('512 MiB'));
    expect(await db.loadRanked(), isEmpty);
  });

  test('changed source after preview is rejected at confirmation', () async {
    final candidate = (await store.inspectFiles([png])).single;
    await png.writeAsBytes(base64Decode(
        'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7'));
    final result = await store
        .importFiles([png], expectedHashes: {png.path: candidate.hash!});
    expect(result.added, 0);
    expect(result.skipped, 1);
    expect(await db.loadRanked(), isEmpty);
  });

  test(
      'batch association keeps originals, ignores duplicates, and rolls back on failure',
      () async {
    await store.importFiles([png], groupIds: ['all', 'work']);
    final id = (await db.loadRanked()).single.sticker.id;
    await expectLater(db.attachGroupsMany([id, 'missing-sticker'], ['fun']),
        throwsStateError);
    expect((await db.loadRanked()).single.groupIds, isNot(contains('fun')));
    await db.attachGroupsMany([id, id], ['fun', 'work', 'fun']);
    expect((await db.loadRanked()).single.groupIds,
        containsAll(['all', 'work', 'fun']));
  });

  test('mixed sources only associate QQ images with QQ favorites', () async {
    final gif = await File('${root.path}/qq.gif').writeAsBytes(base64Decode(
        'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7'));
    await store.importFiles([png, gif],
        sourceForFile: (f) =>
            f.path == gif.path ? StickerSource.qq : StickerSource.manual);
    final rows = await db.loadRanked();
    expect(
        rows
            .singleWhere((e) => e.sticker.source == StickerSource.manual)
            .groupIds,
        isNot(contains('qq_favorites')));
    expect(
        rows.singleWhere((e) => e.sticker.source == StickerSource.qq).groupIds,
        contains('qq_favorites'));
  });
}
