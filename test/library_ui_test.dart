import 'dart:convert';
import 'dart:io';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:sticker_manager/models.dart';
import 'package:sticker_manager/services/repository.dart';
import 'package:sticker_manager/services/quick_picker_controller.dart';
import 'package:sticker_manager/services/ranking_service.dart';
import 'package:sticker_manager/ui/app_theme.dart';
import 'package:sticker_manager/ui/grid_metrics.dart';
import 'package:sticker_manager/ui/library_page.dart';
import 'package:sticker_manager/ui/library_toolbar.dart';
import 'package:sticker_manager/ui/sticker_card.dart';
import 'package:sticker_manager/ui/sticker_grid.dart';

class MemoryRepository extends Fake implements StickerRepository {
  MemoryRepository(this.entries);
  List<RankedSticker> entries;
  bool failLoading = false;

  @override
  Future<List<RankedSticker>> loadRanked() async {
    if (failLoading) throw StateError('test load failure');
    return entries;
  }

  @override
  Future<List<StickerGroup>> loadGroups() async => [
        StickerGroup(id: 'all', name: '全部', createdAt: DateTime(2026)),
        StickerGroup(
            id: 'qq_favorites', name: 'QQ收藏', createdAt: DateTime(2026)),
      ];

  @override
  Future<void> recordUsage(String id, DateTime usedAt) async {
    entries = entries
        .map((entry) => entry.sticker.id == id
            ? RankedSticker(
                entry.sticker.copyWith(
                    usageCount: entry.sticker.usageCount + 1,
                    lastUsedAt: usedAt),
                entry.groupIds)
            : entry)
        .toList();
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Directory directory;
  late MemoryRepository repository;
  late String imagePath;

  setUpAll(() async {
    directory = await Directory.systemTemp.createTemp('sticker-ui-test-');
    addTearDown(() => directory.delete(recursive: true));
    imagePath = '${directory.path}/sample.png';
    await File(imagePath).writeAsBytes(base64Decode(
        'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aN1cAAAAASUVORK5CYII='));
  });

  setUp(() {
    SharedPreferences.setMockInitialValues({});
    repository = MemoryRepository([
      for (final entry in [('old', 10, 1), ('new', 0, 2)])
        RankedSticker(
            Sticker(
              id: entry.$1,
              hash: entry.$1,
              mediaType: StickerMediaType.image,
              filePath: imagePath,
              thumbnailPath: imagePath,
              thumbnailVersion: 2,
              source: StickerSource.manual,
              note: entry.$1,
              createdAt: DateTime(2026, 1, entry.$3),
              updatedAt: DateTime(2026),
              usageCount: entry.$2,
            ),
            {'all', if (entry.$1 == 'old') 'qq_favorites'}),
    ]);
  });

  Widget shell(Widget child,
          {TargetPlatform platform = TargetPlatform.android}) =>
      MaterialApp(
        theme: AppTheme.themeData().copyWith(platform: platform),
        home: Scaffold(body: Column(children: [child])),
      );

  void viewport(WidgetTester tester, Size size) {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = size;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
  }

  Future<void> loadPage(WidgetTester tester,
      {double topInset = 0, bool expectLoaded = true}) async {
    await tester.pumpWidget(MaterialApp(
      // Exercise the touch card layout on every host, including macOS CI.
      theme: AppTheme.themeData().copyWith(platform: TargetPlatform.android),
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(padding: EdgeInsets.only(top: topInset)),
        child: child!,
      ),
      home: LibraryPage(repository: repository),
    ));
    await tester.pumpAndSettle();
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();
    });
    if (expectLoaded) expect(find.byType(StickerGrid), findsOneWidget);
  }

  testWidgets('Android toolbar fits 360dp without layout overflow',
      (tester) async {
    viewport(tester, const Size(360, 800));
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    await tester.pumpWidget(shell(LibraryToolbar(
      searchController: controller,
      onSearchChanged: (_) {},
      onClearSearch: () {},
      onEnterSelection: () {},
      onImport: () {},
      fixedControlHeight: false,
      leading: IconButton(onPressed: () {}, icon: const Icon(Icons.menu)),
      moreMenu: const SizedBox(width: 38, height: 38),
    )));
    expect(tester.takeException(), isNull);
    expect(tester.getSize(find.byType(TextField)).width,
        greaterThanOrEqualTo(100));
  });

  testWidgets('Android selection toolbar fits 360dp', (tester) async {
    viewport(tester, const Size(360, 800));
    await tester.pumpWidget(shell(SelectionToolbar(
      selectedCount: 2,
      onSelectAll: () {},
      onMove: () {},
      onDelete: () {},
      onExit: () {},
    )));
    expect(tester.takeException(), isNull);
  });

  testWidgets('changing sort preserves focused sticker identity',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    final before = tester.widget<StickerGrid>(find.byType(StickerGrid));
    final focusedId = before.stickers[before.focusedIndex].sticker.id;
    expect(focusedId, 'old');
    await tester.tap(find.byType(DropdownButton<StickerSortOrder>));
    await tester.pumpAndSettle();
    await tester.tap(find.text('最近导入').last);
    await tester.pumpAndSettle();
    final after = tester.widget<StickerGrid>(find.byType(StickerGrid));
    expect(after.stickers[after.focusedIndex].sticker.id, focusedId);
  });

  testWidgets('management toolbar respects top system inset', (tester) async {
    viewport(tester, const Size(720, 800));
    await loadPage(tester, topInset: 24);
    expect(tester.getTopLeft(find.byType(LibraryToolbar)).dy,
        greaterThanOrEqualTo(24));
  });

  testWidgets('focused search keeps arrow navigation in the input',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    await tester.tap(find.byType(TextField));
    await tester.pump();
    final before =
        tester.widget<StickerGrid>(find.byType(StickerGrid)).focusedIndex;
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pumpAndSettle();
    expect(tester.widget<StickerGrid>(find.byType(StickerGrid)).focusedIndex,
        before);
  });

  testWidgets('Enter while searching does not copy or send', (tester) async {
    viewport(tester, const Size(1100, 760));
    var copies = 0;
    const channel = MethodChannel('sticker_manager/platform');
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
      if (call.method == 'copySticker') {
        copies++;
        return true;
      }
      return null;
    });
    addTearDown(() => TestDefaultBinaryMessengerBinding
        .instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null));
    await loadPage(tester);
    await tester.enterText(find.byType(TextField), 'o');
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)));
    await tester.pump();
    expect(copies, 0);
  }, skip: !(Platform.isMacOS || Platform.isWindows));
  testWidgets('quick picker displays load failure and retries', (tester) async {
    viewport(tester, const Size(760, 600));
    repository.failLoading = true;
    await loadPage(tester, expectLoaded: false);
    QuickPickerController.instance.onModeChanged?.call(QuickPickerMode.quick);
    await tester.pumpAndSettle();
    expect(find.textContaining('test load failure'), findsOneWidget);
    expect(find.text('还没有表情包'), findsNothing);
    repository.failLoading = false;
    await tester.tap(find.text('重试'));
    await tester.pumpAndSettle();
    expect(find.byType(StickerGrid), findsOneWidget);
    expect(find.textContaining('初始化失败'), findsNothing);
  }, skip: !(Platform.isMacOS || Platform.isWindows));

  testWidgets('group totals remain independent of search results',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    expect(find.text('2 张表情'), findsOneWidget);
    await tester.tap(find.text('QQ收藏'));
    await tester.pumpAndSettle();
    expect(find.text('1 张表情'), findsOneWidget);
    await tester.enterText(find.byType(TextField), 'missing');
    await tester.pumpAndSettle();
    expect(find.text('搜索到 0 个表情'), findsOneWidget);
    expect(find.text('1'), findsOneWidget);
    expect(find.text('2'), findsOneWidget);
  });

  testWidgets('hover copy button does not invoke card use', (tester) async {
    viewport(tester, const Size(1100, 760));
    var copies = 0;
    var uses = 0;
    final entry = repository.entries.first;
    await tester.pumpWidget(shell(
        SizedBox(
          width: 200,
          height: 204,
          child: StickerCard(
              entry: entry,
              selectionMode: false,
              selected: false,
              keyboardFocused: false,
              onUse: () => uses++,
              onSelect: () {},
              onCopy: () => copies++,
              onLongPress: null,
              onPin: () {},
              onGroups: () {},
              onEdit: () {},
              onDelete: () {},
              onContextMenu: (_) async {},
              compact: false),
        ),
        platform: TargetPlatform.macOS));
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    await mouse.moveTo(tester.getCenter(find.byType(StickerCard)));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('复制'));
    await tester.pump(const Duration(milliseconds: 700));
    expect(copies, 1);
    expect(uses, 0);
  });

  testWidgets('Android narrow cards keep all actions without triggering use',
      (tester) async {
    viewport(tester, const Size(360, 800));
    final actions = <String>[];
    var uses = 0;
    final entry = repository.entries.first;
    final longNote = RankedSticker(
      entry.sticker.copyWith(note: '这是一条用于检查窄卡片布局的较长备注'),
      entry.groupIds,
    );
    for (final width in [132.0, 152.6]) {
      await tester.pumpWidget(shell(SizedBox(
        width: width,
        height: width + 32,
        child: StickerCard(
          entry: longNote,
          selectionMode: false,
          selected: false,
          keyboardFocused: false,
          compact: false,
          onUse: () => uses++,
          onSelect: () {},
          onCopy: () {},
          onLongPress: null,
          onPin: () => actions.add('置顶'),
          onGroups: () => actions.add('管理分组'),
          onEdit: () => actions.add('编辑备注'),
          onDelete: () => actions.add('删除表情'),
          onContextMenu: (_) async {},
        ),
      )));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.byTooltip('表情操作'), findsOneWidget);
      for (final label in ['管理分组', '编辑备注', '删除表情']) {
        await tester.tap(find.byTooltip('表情操作'));
        await tester.pump(const Duration(milliseconds: 350));
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull);
        await tester.tap(find.text(label));
        await tester.pump(const Duration(milliseconds: 700));
        await tester.pumpAndSettle();
        expect(actions.removeLast(), label);
        expect(actions, isEmpty);
        expect(uses, 0);
      }
      await tester.tap(find.byTooltip('置顶'));
      await tester.pump(const Duration(milliseconds: 700));
      await tester.pumpAndSettle();
      expect(actions.removeLast(), '置顶');
      expect(uses, 0);
    }
  });

  testWidgets('selected cards retain an independent keyboard focus indicator',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    Finder outline() => find.descendant(
        of: find.byType(StickerCard),
        matching: find.byWidgetPredicate((widget) =>
            widget is Container && widget.foregroundDecoration != null));
    expect(outline(), findsNothing);
    await tester.tap(find.text('多选'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('全选当前列表'));
    await tester.pumpAndSettle();
    final listener =
        tester.widget<KeyboardListener>(find.byType(KeyboardListener));
    listener.focusNode.requestFocus();
    await tester.pump();
    expect(outline(), findsOneWidget);
    final before = tester.getTopLeft(outline());
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pumpAndSettle();
    expect(outline(), findsOneWidget);
    expect(tester.getTopLeft(outline()), isNot(before));
    await tester.tap(find.byTooltip('退出多选'));
    await tester.pumpAndSettle();
    await tester.tap(find.byType(TextField));
    await tester.pumpAndSettle();
    expect(outline(), findsNothing);
  });
  for (final width in [360.0, 720.0, 1100.0, 1440.0, 1536.0]) {
    testWidgets('management layout fits width $width', (tester) async {
      viewport(tester, Size(width, 900));
      await loadPage(tester);
      expect(tester.takeException(), isNull);
      await tester.tap(find.text('多选'));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets(
      'grid hit testing agrees with rendered cards after scroll and density changes',
      (tester) async {
    viewport(tester, const Size(720, 520));
    final scroll = ScrollController();
    addTearDown(scroll.dispose);
    GridMetrics? metrics;
    Set<String> selected = {};
    var uses = 0;
    final entries = [
      for (var i = 0; i < 24; i++)
        RankedSticker(
            Sticker(
                id: 's$i',
                hash: 's$i',
                mediaType: StickerMediaType.image,
                filePath: imagePath,
                thumbnailPath: imagePath,
                thumbnailVersion: 2,
                source: StickerSource.manual,
                note: '$i',
                createdAt: DateTime(2026),
                updatedAt: DateTime(2026)),
            {'all'}),
    ];
    for (final density in GridDensity.values) {
      await tester.pumpWidget(MaterialApp(
          theme: AppTheme.themeData(),
          home: Scaffold(
            body: StickerGrid(
                stickers: entries,
                density: density,
                quickPicker: false,
                selectionMode: true,
                selectedIds: selected,
                focusedIndex: -1,
                scrollController: scroll,
                onMetricsChanged: (value) => metrics = value,
                onUse: (_) => uses++,
                onSelect: (entry) => selected.add(entry.sticker.id),
                onDragSelection: (ids) => selected = ids,
                onExitSelection: () {},
                onCopy: (_) {},
                onLongPress: null,
                onPin: (_) {},
                onGroups: (_) {},
                onEdit: (_) {},
                onDelete: (_) {},
                onContextMenu: (_, __) async {}),
          )));
      await tester.pumpAndSettle();
      scroll.jumpTo(metrics!.rowExtent);
      await tester.pumpAndSettle();
      final firstIndex = metrics!.columnCount;
      final firstRect = tester.getRect(find.byKey(ValueKey('s$firstIndex')));
      expect(firstRect,
          metrics!.tileRect(firstIndex, scrollOffset: scroll.offset));
      final secondRect =
          tester.getRect(find.byKey(ValueKey('s${firstIndex + 1}')));
      final gesture = await tester.startGesture(
          firstRect.topLeft + const Offset(30, 55),
          kind: PointerDeviceKind.mouse);
      await gesture.moveTo(secondRect.bottomRight - const Offset(10, 30));
      await gesture.up();
      await tester.pump(const Duration(milliseconds: 700));
      expect(selected, {'s$firstIndex', 's${firstIndex + 1}'});
      expect(uses, 0);
    }
    await tester.pumpWidget(const SizedBox.shrink());
  }, skip: !(Platform.isMacOS || Platform.isWindows));
  testWidgets('copy-driven ranking refresh keeps the focused sticker',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    repository.entries = repository.entries
        .map((entry) => RankedSticker(
            entry.sticker.copyWith(usageCount: 0), entry.groupIds))
        .toList();
    const channel = MethodChannel('sticker_manager/platform');
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel,
            (call) async => call.method == 'copySticker' ? true : null);
    addTearDown(() => TestDefaultBinaryMessengerBinding
        .instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null));
    await loadPage(tester);
    tester
        .widget<KeyboardListener>(find.byType(KeyboardListener))
        .focusNode
        .requestFocus();
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pumpAndSettle();
    final before = tester.widget<StickerGrid>(find.byType(StickerGrid));
    expect(before.stickers[before.focusedIndex].sticker.id, 'old');
    before.onCopy(before.stickers[before.focusedIndex]);
    await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)));
    await tester.pumpAndSettle();
    final after = tester.widget<StickerGrid>(find.byType(StickerGrid));
    expect(after.stickers.first.sticker.id, 'old');
    expect(after.stickers[after.focusedIndex].sticker.id, 'old');
  }, skip: !Platform.isMacOS);
}
