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
import 'package:sticker_manager/ui/filter_panel.dart';
import 'package:sticker_manager/ui/hotkey_setting.dart';
import 'package:hotkey_manager/hotkey_manager.dart';
import 'package:sticker_manager/ui/recent_stickers.dart';
import 'package:sticker_manager/ui/import_preview_dialog.dart';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:sticker_manager/ui/sticker_card.dart';
import 'package:sticker_manager/ui/sticker_grid.dart';

class MemoryRepository extends Fake implements StickerRepository {
  MemoryRepository(this.entries);
  List<RankedSticker> entries;
  bool failLoading = false;
  final extraGroups = <StickerGroup>[];
  @override
  Future<void> createGroup(String id, String name) async {
    extraGroups
        .add(StickerGroup(id: id, name: name, createdAt: DateTime.now()));
  }

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
        ...extraGroups,
      ];

  @override
  Future<void> attachGroupsMany(
      Iterable<String> ids, Iterable<String> groups) async {
    entries = entries
        .map((e) => ids.contains(e.sticker.id)
            ? RankedSticker(e.sticker, {...e.groupIds, ...groups})
            : e)
        .toList();
  }

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

  Future<void> loadPage(
    WidgetTester tester, {
    double topInset = 0,
    bool expectLoaded = true,
    TargetPlatform platform = TargetPlatform.android,
  }) async {
    await tester.pumpWidget(
      MaterialApp(
        // Exercise the touch card layout on every host, including macOS CI.
        theme: AppTheme.themeData().copyWith(platform: platform),
        builder: (context, child) => MediaQuery(
          data: MediaQuery.of(context)
              .copyWith(padding: EdgeInsets.only(top: topInset)),
          child: child!,
        ),
        home: LibraryPage(repository: repository),
      ),
    );
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

  testWidgets('global search retains query and group switch restores scope', (
    tester,
  ) async {
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    await tester.tap(find.text('QQ收藏'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'NEW');
    await tester.pumpAndSettle();
    expect(find.text('没有匹配的表情'), findsOneWidget);
    await tester.tap(find.text('搜索全部表情'));
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<StickerGrid>(find.byType(StickerGrid))
          .stickers
          .single
          .sticker
          .id,
      'new',
    );
    expect(find.text('全部表情'), findsWidgets);
    expect(
      tester.widget<LibraryToolbar>(find.byType(LibraryToolbar)).searchAll,
      isTrue,
    );
    await tester.tap(find.text('QQ收藏'));
    await tester.pumpAndSettle();
    expect(
      tester.widget<LibraryToolbar>(find.byType(LibraryToolbar)).searchAll,
      isFalse,
    );
    expect(
      tester.widget<TextField>(find.byType(TextField)).controller!.text,
      'NEW',
    );
    expect(find.text('没有匹配的表情'), findsOneWidget);
  });

  testWidgets('filter draft, reset, and search have independent lifetimes', (
    tester,
  ) async {
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    await tester.tap(find.text('筛选'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('GIF'));
    await tester.tap(find.byTooltip('关闭筛选'));
    await tester.pumpAndSettle();
    expect(find.byType(StickerGrid), findsOneWidget);
    expect(find.text('筛选 · 1'), findsNothing);

    await tester.tap(find.text('筛选'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('GIF'));
    await tester.tap(find.text('应用'));
    await tester.pumpAndSettle();
    expect(find.text('搜索到 0 个表情'), findsOneWidget);
    expect(find.text('还没有表情包'), findsNothing);
    expect(find.text('筛选 · 1'), findsOneWidget);

    await tester.tap(find.text('筛选 · 1'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('重置'));
    await tester.tap(find.byTooltip('关闭筛选'));
    await tester.pumpAndSettle();
    expect(find.text('筛选 · 1'), findsOneWidget);
    await tester.enterText(find.byType(TextField), 'old');
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('清除搜索'));
    await tester.pumpAndSettle();
    expect(find.text('筛选 · 1'), findsOneWidget);
    await tester.enterText(find.byType(TextField), 'new');
    await tester.pumpAndSettle();
    await tester.tap(find.text('清除筛选').first);
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<StickerGrid>(find.byType(StickerGrid))
          .stickers
          .single
          .sticker
          .id,
      'new',
    );
  });

  testWidgets('group switch retains filters and relaunch clears them', (
    tester,
  ) async {
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    await tester.tap(find.text('筛选'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('仅置顶'));
    await tester.tap(find.text('应用'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('QQ收藏'));
    await tester.pumpAndSettle();
    expect(find.text('筛选 · 1'), findsOneWidget);
    expect(find.text('搜索到 0 个表情'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    await loadPage(tester);
    expect(find.text('筛选 · 1'), findsNothing);
    expect(find.text('2 张表情'), findsOneWidget);
  });

  testWidgets('filter layouts fit phone, short desktop and wide desktop', (
    tester,
  ) async {
    for (final entry in [
      (const Size(360, 640), TargetPlatform.android),
      (const Size(720, 420), TargetPlatform.macOS),
      (const Size(1440, 900), TargetPlatform.windows),
    ]) {
      viewport(tester, entry.$1);
      await loadPage(tester, platform: entry.$2);
      await tester.tap(find.text('筛选'));
      await tester.pumpAndSettle();
      expect(find.byType(FilterPanel), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.tap(find.text('GIF'));
      await tester.tap(find.text('应用'));
      await tester.pumpAndSettle();
      expect(find.byType(FilterPanel), findsNothing);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    }
  });

  testWidgets('quick picker exposes and clears management filters', (
    tester,
  ) async {
    viewport(tester, const Size(760, 600));
    await loadPage(tester);
    await tester.tap(find.text('筛选'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('GIF'));
    await tester.tap(find.text('应用'));
    await tester.pumpAndSettle();
    QuickPickerController.instance.onModeChanged?.call(QuickPickerMode.quick);
    await tester.pumpAndSettle();
    expect(find.text('筛选 · 1'), findsOneWidget);
    expect(find.text('没有匹配的表情'), findsOneWidget);
    await tester.tap(find.text('清除筛选').first);
    await tester.pumpAndSettle();
    expect(find.byType(StickerGrid), findsOneWidget);
  }, skip: !(Platform.isMacOS || Platform.isWindows));

  testWidgets(
      'recent row is global, hidden by search and selection, and configurable',
      (tester) async {
    SharedPreferences.setMockInitialValues({
      'recentStickerUses': [
        jsonEncode({'id': 'new', 'at': '2026-09-20'})
      ]
    });
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    expect(find.byType(RecentStickers), findsOneWidget);
    await tester.tap(find.text('QQ收藏'));
    await tester.pumpAndSettle();
    expect(
        tester
            .widget<RecentStickers>(find.byType(RecentStickers))
            .entries
            .single
            .sticker
            .id,
        'new');
    await tester.enterText(find.byType(TextField), 'old');
    await tester.pumpAndSettle();
    expect(find.byType(RecentStickers), findsNothing);
    await tester.tap(find.byTooltip('清除搜索'));
    await tester.tap(find.text('多选'));
    await tester.pumpAndSettle();
    expect(find.byType(RecentStickers), findsNothing);
    await tester.tap(find.byTooltip('退出多选'));
    await tester.tap(find.text('设置'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('显示最近使用'));
    await tester.pumpAndSettle();
    expect(find.byType(RecentStickers), findsNothing);
    expect(
        (await SharedPreferences.getInstance()).getBool('showRecent'), isFalse);
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(tester.widget<SwitchListTile>(find.byType(SwitchListTile)).value,
        isFalse);
    await tester.tap(find.text('显示最近使用'));
    await tester.pumpAndSettle();
    expect(tester.widget<SwitchListTile>(find.byType(SwitchListTile)).value,
        isTrue);
    expect(
        (await SharedPreferences.getInstance()).getBool('showRecent'), isTrue);
    await tester.tap(find.text('关闭'));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
    expect(find.byType(RecentStickers), findsOneWidget);
  });

  testWidgets('settings edits hotkey inline without opening a second dialog',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    await tester.tap(find.text('设置'));
    await tester.pumpAndSettle();
    expect(find.text('快速唤出快捷键'), findsOneWidget);
    final editButton = find.descendant(
        of: find.byType(HotkeySetting), matching: find.byType(OutlinedButton));
    final closeButton = find.widgetWithText(FilledButton, '关闭');
    final rightEdge = tester.getRect(closeButton).right;
    expect(tester.getRect(editButton).right, closeTo(rightEdge, 0.01));
    expect(tester.getRect(find.byType(Switch)).right, closeTo(rightEdge, 0.01));
    await tester.tap(find.descendant(
        of: find.byType(HotkeySetting), matching: find.byType(OutlinedButton)));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(find.byType(HotKeyRecorder), findsOneWidget);
    expect(tester.getRect(find.byKey(const ValueKey('hotkey-editor'))).right,
        closeTo(rightEdge, 0.01));
    await tester.tap(find.text('取消'));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(find.text('显示最近使用'), findsOneWidget);
    await tester.tap(find.text('关闭'));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
  }, skip: !(Platform.isMacOS || Platform.isWindows));

  testWidgets('inline hotkey validates draft and keeps editing on failure',
      (tester) async {
    viewport(tester, const Size(600, 400));
    var calls = 0;
    var succeed = false;
    final current = QuickPickerController.instance.hotKey;
    await tester.pumpWidget(shell(
        HotkeySetting(
            current: current,
            onSave: (_) async {
              calls++;
              return succeed;
            }),
        platform: TargetPlatform.macOS));
    await tester.tap(find.byType(OutlinedButton));
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
    await tester.pump();
    await tester.tap(find.text('保存'));
    await tester.pumpAndSettle();
    expect(calls, 0);
    expect(find.text('请使用包含修饰键和普通按键的组合键'), findsOneWidget);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();
    await tester.tap(find.text('保存'));
    await tester.pumpAndSettle();
    expect(calls, 1);
    expect(find.byType(HotKeyRecorder), findsOneWidget);
    expect(find.text('无法保存，快捷键可能已被占用，请换一组重试'), findsOneWidget);
    succeed = true;
    await tester.tap(find.text('保存'));
    await tester.pumpAndSettle();
    expect(calls, 2);
    expect(find.byType(HotKeyRecorder), findsNothing);
    expect(find.text('快捷键已保存'), findsOneWidget);
  });

  testWidgets(
      'desktop images fill square cards and short recent row hides paging',
      (tester) async {
    SharedPreferences.setMockInitialValues({
      'recentStickerUses': [
        jsonEncode({'id': 'new', 'at': '2026-09-20'})
      ]
    });
    viewport(tester, const Size(1100, 760));
    await loadPage(tester, platform: TargetPlatform.macOS);
    final recentImage = find
        .descendant(
            of: find.byType(RecentStickers), matching: find.byType(Image))
        .first;
    final gridImage = find
        .descendant(of: find.byType(StickerGrid), matching: find.byType(Image))
        .first;
    expect(tester.getSize(recentImage), const Size(72, 72));
    final cardSize = tester.getSize(find.byType(StickerCard).first);
    expect(cardSize.width, closeTo(cardSize.height, 0.01));
    // Four pixels of padding plus the one-pixel border on each side.
    expect(tester.getSize(gridImage).width, closeTo(cardSize.width - 10, 0.01));
    expect(
        tester.getSize(gridImage).height, closeTo(cardSize.height - 10, 0.01));
    expect(tester.widget<Image>(gridImage).fit, BoxFit.contain);
    expect(find.byTooltip('下一页最近使用'), findsNothing);
  });

  testWidgets('recent paging follows overflow and scroll position',
      (tester) async {
    viewport(tester, const Size(360, 240));
    await tester.pumpWidget(MaterialApp(
      theme: AppTheme.themeData().copyWith(platform: TargetPlatform.macOS),
      home: Scaffold(
          body: RecentStickers(
        entries: List.generate(10, (_) => repository.entries.first),
        onUse: (_) {},
        onCopy: (_) {},
      )),
    ));
    await tester.pumpAndSettle();
    expect(
        tester
            .widget<IconButton>(find.byWidgetPredicate((widget) =>
                widget is IconButton && widget.tooltip == '上一页最近使用'))
            .onPressed,
        isNull);
    expect(
        tester
            .widget<IconButton>(find.byWidgetPredicate((widget) =>
                widget is IconButton && widget.tooltip == '下一页最近使用'))
            .onPressed,
        isNotNull);
    await tester.tap(find.byTooltip('下一页最近使用'));
    await tester.pumpAndSettle();
    expect(
        tester
            .widget<IconButton>(find.byWidgetPredicate((widget) =>
                widget is IconButton && widget.tooltip == '上一页最近使用'))
            .onPressed,
        isNotNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('successful copy records recent; failed copy does not',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    var succeed = false;
    var nativeCalls = 0;
    const channel = MethodChannel('sticker_manager/platform');
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(channel, (call) async {
      if (call.method == 'copySticker') {
        nativeCalls++;
        return succeed;
      }
      return null;
    });
    messenger.setMockMethodCallHandler(const MethodChannel('pasteboard'),
        (_) async => throw PlatformException(code: 'unavailable'));
    addTearDown(() {
      messenger.setMockMethodCallHandler(channel, null);
      messenger.setMockMethodCallHandler(
          const MethodChannel('pasteboard'), null);
    });
    await loadPage(tester);
    var grid = tester.widget<StickerGrid>(find.byType(StickerGrid));
    await tester.runAsync(() async {
      grid.onCopy(RankedSticker(
          Sticker(
              id: 'missing',
              hash: 'missing',
              mediaType: StickerMediaType.image,
              filePath: '${directory.path}/missing.png',
              thumbnailPath: '',
              source: StickerSource.manual,
              createdAt: DateTime(2026),
              updatedAt: DateTime(2026)),
          {'all'}));
      await Future<void>.delayed(const Duration(milliseconds: 450));
    });
    await tester.pumpAndSettle();
    expect(find.byType(RecentStickers), findsNothing);
    await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 400)));
    expect(find.textContaining('无法写入'), findsWidgets);
    succeed = true;
    grid = tester.widget<StickerGrid>(find.byType(StickerGrid));
    await tester.runAsync(() async {
      grid.onCopy(grid.stickers.first);
      await Future<void>.delayed(const Duration(milliseconds: 100));
    });
    await tester.pumpAndSettle();
    await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)));
    await tester.pumpAndSettle();
    expect(nativeCalls, 1, reason: 'success action must reach native copy');
    expect(
        (await SharedPreferences.getInstance())
            .getStringList('recentStickerUses'),
        isNotEmpty);
    expect(
        tester
            .widget<RecentStickers>(find.byType(RecentStickers))
            .entries
            .single
            .sticker
            .id,
        'old');
    if (Platform.isWindows) {
      expect(repository.entries.first.sticker.usageCount, 10);
    }
  }, skip: !(Platform.isMacOS || Platform.isWindows));

  testWidgets('batch addition retains selection and original memberships',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    await tester.tap(find.text('多选'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('全选当前列表'));
    await tester.pump();
    await tester.tap(find.text('添加到分组'));
    await tester.pumpAndSettle();
    expect(find.text('已有 1/2 张'), findsOneWidget);
    await tester.tap(find.widgetWithText(CheckboxListTile, 'QQ收藏'));
    await tester.pump();
    await tester.tap(find.text('添加到 1 个分组'));
    await tester.pumpAndSettle();
    expect(
        tester
            .widget<SelectionToolbar>(find.byType(SelectionToolbar))
            .selectedCount,
        2);
    expect(
        repository.entries
            .every((e) => e.groupIds.containsAll({'all', 'qq_favorites'})),
        isTrue);
  });

  testWidgets('new group in batch dialog is selected automatically',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    await tester.tap(find.text('多选'));
    await tester.pump();
    await tester.tap(find.text('全选当前列表'));
    await tester.pump();
    await tester.tap(find.text('添加到分组'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(TextButton, '新建分组'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).last, '新收藏');
    await tester.tap(find.text('创建'));
    await tester.pumpAndSettle();
    expect(find.text('添加到 1 个分组'), findsOneWidget);
    expect(
        tester
            .widget<CheckboxListTile>(
                find.widgetWithText(CheckboxListTile, '新收藏'))
            .value,
        isTrue);
    await tester.tap(find.text('添加到 1 个分组'));
    await tester.pumpAndSettle();
    final id = repository.extraGroups.single.id;
    expect(repository.entries.every((e) => e.groupIds.contains(id)), isTrue);
  });

  testWidgets('paste shortcut respects text fields and modal focus',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    var reads = 0;
    const clipboard = MethodChannel('pasteboard');
    const platform = MethodChannel('sticker_manager/platform');
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(clipboard, (call) async {
      if (call.method == 'files') {
        reads++;
        return [];
      }
      return null;
    });
    messenger.setMockMethodCallHandler(platform, (_) async => null);
    addTearDown(() {
      messenger.setMockMethodCallHandler(clipboard, null);
      messenger.setMockMethodCallHandler(platform, null);
    });
    Future<void> paste() async {
      final modifier = Platform.isMacOS
          ? LogicalKeyboardKey.metaLeft
          : LogicalKeyboardKey.controlLeft;
      await tester.sendKeyDownEvent(modifier);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyV);
      await tester.sendKeyUpEvent(modifier);
      await tester.pumpAndSettle();
    }

    await loadPage(tester);
    await tester.tap(find.byType(TextField));
    await paste();
    expect(reads, 0);
    tester
        .widget<KeyboardListener>(find.byType(KeyboardListener))
        .focusNode
        .requestFocus();
    await tester.pump();
    await paste();
    expect(reads, 1);
    await tester.tap(find.text('筛选'));
    await tester.pumpAndSettle();
    await paste();
    expect(reads, 1);
  }, skip: !(Platform.isMacOS || Platform.isWindows));

  testWidgets(
      'drop opens shared preview, cancel preserves originals and library',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    await loadPage(tester);
    final target = tester.widget<DropTarget>(find.byType(DropTarget));
    await tester.runAsync(() async {
      target.onDragDone!(DropDoneDetails(
          files: [DropItemFile(imagePath)],
          localPosition: Offset.zero,
          globalPosition: Offset.zero));
      await Future<void>.delayed(const Duration(milliseconds: 150));
    });
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.byType(ImportPreviewDialog), findsOneWidget);
    await tester.tap(find.text('取消'));
    await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)));
    await tester.pumpAndSettle();
    expect(repository.entries.length, 2);
    expect(File(imagePath).existsSync(), isTrue);
    QuickPickerController.instance.onModeChanged?.call(QuickPickerMode.quick);
    await tester.pumpAndSettle();
    expect(find.byType(DropTarget), findsNothing);
  }, skip: !(Platform.isMacOS || Platform.isWindows));

  testWidgets('desktop note is hidden until hovering sticker image',
      (tester) async {
    viewport(tester, const Size(1100, 760));
    const note = '这是一段超过卡片宽度的完整备注，用来确认图片悬停时不会截断备注内容。';
    final entry = repository.entries.first;
    repository.entries = [
      RankedSticker(entry.sticker.copyWith(note: note), entry.groupIds)
    ];
    await loadPage(tester, platform: TargetPlatform.macOS);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    expect(find.text(note, findRichText: true), findsNothing);
    final image = find
        .descendant(of: find.byType(StickerCard), matching: find.byType(Image))
        .first;
    await mouse.moveTo(tester.getCenter(image));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 700));
    expect(find.text(note, findRichText: true), findsOneWidget);
    await mouse.moveTo(const Offset(1, 1));
    await tester.pumpAndSettle();
    expect(find.text(note, findRichText: true), findsNothing);
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
