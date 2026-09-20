import 'dart:async';
import 'dart:io';
import 'package:desktop_drop/desktop_drop.dart';
import 'dart:math' as math;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path/path.dart' as path;

import '../models.dart';
import '../platform/desktop_platform.dart';
import '../platform/platform_bridge.dart';
import '../platform/clipboard_bridge.dart';
import '../platform/desktop_drop_bridge.dart';
import '../services/import_preview.dart';
import 'import_preview_dialog.dart';
import '../services/database.dart';
import '../services/export_service.dart';
import '../services/import_source.dart';
import '../services/media_store.dart';
import '../services/preferences.dart';
import '../services/quick_picker_controller.dart';
import '../services/ranking_service.dart';
import '../services/repository.dart';
import '../services/recent_usage.dart';
import 'recent_stickers.dart';
import 'add_to_groups_dialog.dart';
import '../services/sticker_filter.dart';
import 'app_theme.dart';
import 'filter_panel.dart';
import 'hotkey_setting.dart';
import 'grid_metrics.dart';
import 'library_feedback.dart';
import 'library_sidebar.dart';
import 'library_toolbar.dart';
import 'sticker_grid.dart';

/// Coordinates library state (loading, groups, search, sorting, selection)
/// and business actions (import, export, clipboard use). Presentation is
/// delegated to the components under `lib/ui/`; this widget owns the data
/// and the callbacks they invoke.
class LibraryPage extends StatefulWidget {
  const LibraryPage({super.key, this.repository});

  final StickerRepository? repository;

  @override
  State<LibraryPage> createState() => _LibraryPageState();
}

class _LibraryPageState extends State<LibraryPage> with WidgetsBindingObserver {
  late final StickerRepository _repository =
      widget.repository ?? StickerDatabase();
  late final MediaStore _mediaStore = MediaStore(_repository);
  late final ExportPackageService _exportService =
      ExportPackageService(_repository);
  final _ranking = UsageRankingService();
  final _preferences = AppPreferences();
  final _recentUsage = RecentUsage();
  List<String> _recentIds = [];
  bool _showRecent = true;
  bool _stickerActionBusy = false;
  DateTime? _lastStickerAction;
  String? _lastStickerActionId;
  final _searchController = TextEditingController();
  final _gridFocusNode = FocusNode(debugLabel: 'sticker-grid');
  final _gridScrollController = ScrollController();
  final _scaffoldKey = GlobalKey<ScaffoldState>();
  List<RankedSticker> _stickers = [];
  List<StickerGroup> _groups = [];
  String? _selectedGroup;
  bool _searchAll = false;
  StickerFilter _filter = const StickerFilter();
  bool _loading = true;
  bool _importing = false;
  bool _dragging = false;
  String _importStatus = '';
  bool _selectionMode = false;
  final Set<String> _selectedStickerIds = <String>{};
  String? _error;
  bool _thumbnailMigrationStarted = false;
  StreamSubscription<List<String>>? _sharedFilesSubscription;
  bool _sharedImportQueued = false;
  bool _floatingPanelEnabled = false;
  bool _floatingUsageSyncInProgress = false;
  bool _quickPickerMode = false;
  GridDensity _density = GridDensity.standard;
  StickerSortOrder _sortOrder = StickerSortOrder.defaultRule;
  int _focusedStickerIndex = -1;
  String? _focusedStickerId;
  GridMetrics _gridMetrics = GridMetrics.resolve(availableWidth: 0);
  String? _feedbackMessage;
  Timer? _feedbackTimer;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    QuickPickerController.instance.onPickerShown = _focusQuickPicker;
    _quickPickerMode = isDesktopPlatform &&
        QuickPickerController.instance.mode == QuickPickerMode.quick;
    QuickPickerController.instance.onModeChanged = _handlePickerModeChanged;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted ||
          !QuickPickerController.instance.hotKeyRegistrationUnavailable) {
        return;
      }
      _showMessage('快速唤出热键已被其他程序占用，请在设置中更换热键');
    });
    if (Platform.isAndroid) {
      _sharedFilesSubscription =
          PlatformBridge.instance.sharedFiles.listen((_) {
        unawaited(_onSharedFilesAvailable());
      });
    }
    _initializeLibrary();
  }

  @override
  void dispose() {
    if (QuickPickerController.instance.onPickerShown == _focusQuickPicker) {
      QuickPickerController.instance.onPickerShown = null;
    }
    QuickPickerController.instance.onModeChanged = null;
    WidgetsBinding.instance.removeObserver(this);
    _sharedFilesSubscription?.cancel();
    _feedbackTimer?.cancel();
    _searchController.dispose();
    _gridFocusNode.dispose();
    _gridScrollController.dispose();
    super.dispose();
  }

  void _handlePickerModeChanged(QuickPickerMode mode) {
    if (!mounted) return;
    setState(() {
      _quickPickerMode = isDesktopPlatform && mode == QuickPickerMode.quick;
      // Quick mode is a send-only surface. A selection left over from the
      // management window must not turn its first click into another toggle.
      if (mode == QuickPickerMode.quick) {
        _selectionMode = false;
        _selectedStickerIds.clear();
      }
    });
    _resetKeyboardFocus();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed && Platform.isAndroid) {
      unawaited(_consumeFloatingUsage());
      unawaited(_refreshFloatingPanelState());
    }
  }

  void _focusQuickPicker() {
    if (!mounted) return;
    _resetKeyboardFocus();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || _visibleStickers.isEmpty) return;
      _gridFocusNode.requestFocus();
      _scrollToFocusedSticker(_focusedStickerIndex);
      setState(() {});
    });
  }

  void _resetKeyboardFocus() {
    final visible = _visibleStickers;
    _focusedStickerIndex = visible.isEmpty ? -1 : 0;
    _focusedStickerId = _focusedStickerIndex >= 0
        ? visible[_focusedStickerIndex].sticker.id
        : null;
  }

  void _restoreKeyboardFocus() {
    final visible = _visibleStickers;
    if (visible.isEmpty) {
      _focusedStickerIndex = -1;
      _focusedStickerId = null;
      return;
    }
    final retained =
        visible.indexWhere((entry) => entry.sticker.id == _focusedStickerId);
    _focusedStickerIndex = retained >= 0
        ? retained
        : _focusedStickerIndex.clamp(0, visible.length - 1).toInt();
    _focusedStickerId = visible[_focusedStickerIndex].sticker.id;
  }

  void _handleGridMetricsChanged(GridMetrics metrics) {
    setState(() {
      _gridMetrics = metrics;
      _restoreKeyboardFocus();
    });
    if (_gridFocusNode.hasPrimaryFocus) {
      _scrollToFocusedSticker(_focusedStickerIndex);
    }
  }

  void _handleGridKey(KeyEvent event) {
    if (!_gridFocusNode.hasPrimaryFocus) return;
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return;
    }
    if (event.logicalKey == LogicalKeyboardKey.escape) {
      if (_selectionMode) _exitSelectionMode();
      return;
    }
    final visible = _visibleStickers;
    if (visible.isEmpty) return;
    switch (event.logicalKey) {
      case LogicalKeyboardKey.arrowLeft:
        _moveKeyboardFocus(rowDelta: 0, columnDelta: -1);
        return;
      case LogicalKeyboardKey.arrowRight:
        _moveKeyboardFocus(rowDelta: 0, columnDelta: 1);
        return;
      case LogicalKeyboardKey.arrowUp:
        _moveKeyboardFocus(rowDelta: -1, columnDelta: 0);
        return;
      case LogicalKeyboardKey.arrowDown:
        _moveKeyboardFocus(rowDelta: 1, columnDelta: 0);
        return;
      case LogicalKeyboardKey.enter:
      case LogicalKeyboardKey.numpadEnter:
        final index = _focusedStickerIndex.clamp(0, visible.length - 1).toInt();
        if (_selectionMode) {
          _toggleSelected(visible[index]);
        } else {
          unawaited(_useSticker(visible[index]));
        }
        return;
    }
  }

  void _moveKeyboardFocus({required int rowDelta, required int columnDelta}) {
    final visible = _visibleStickers;
    if (visible.isEmpty) return;
    final current =
        _focusedStickerIndex < 0 || _focusedStickerIndex >= visible.length
            ? 0
            : _focusedStickerIndex;
    final next = (current +
            rowDelta * math.max(1, _gridMetrics.columnCount) +
            columnDelta)
        .clamp(0, visible.length - 1)
        .toInt();
    if (next == _focusedStickerIndex) return;
    setState(() {
      _focusedStickerIndex = next;
      _focusedStickerId = visible[next].sticker.id;
    });
    _scrollToFocusedSticker(next);
  }

  void _scrollToFocusedSticker(int index) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_gridScrollController.hasClients || index < 0) return;
      final rect = _gridMetrics.tileRect(index);
      final position = _gridScrollController.position;
      var target = position.pixels;
      if (rect.top < position.pixels) {
        target = rect.top;
      } else if (rect.bottom > position.pixels + position.viewportDimension) {
        target = rect.bottom - position.viewportDimension;
      }
      target = target.clamp(0.0, position.maxScrollExtent).toDouble();
      if ((target - position.pixels).abs() < 1) return;
      _gridScrollController.animateTo(
        target,
        duration: const Duration(milliseconds: 120),
        curve: Curves.easeOut,
      );
    });
  }

  Future<void> _load() async {
    try {
      final values = await Future.wait([
        _repository.loadRanked(),
        _repository.loadGroups(),
      ]);
      if (!mounted) return;
      final loadedStickers = values[0] as List<RankedSticker>;
      await _recentUsage
          .retain(loadedStickers.map((e) => e.sticker.id).toSet());
      final recentIds = await _recentUsage.ids();
      if (!mounted) return;
      setState(() {
        _recentIds = recentIds;
        _stickers = loadedStickers;
        _groups = values[1] as List<StickerGroup>;
        _loading = false;
        _error = null;
        _restoreKeyboardFocus();
      });
      if (Platform.isAndroid) {
        unawaited(PlatformBridge.instance.syncFloatingPanel(
            _ranking.rank(loadedStickers).map((entry) => entry.sticker)));
      }
    } on Object catch (error) {
      if (mounted) {
        setState(() {
          _loading = false;
          _error = '$error';
        });
      }
    }
  }

  Future<void> _initializeLibrary() async {
    _density = await _preferences.gridDensity();
    _showRecent = await _preferences.showRecent();
    if (!mounted) return;
    await _load();
    if (!mounted || _error != null || _thumbnailMigrationStarted) return;
    if (Platform.isAndroid) {
      unawaited(_refreshFloatingPanelState());
      unawaited(_consumeFloatingUsage());
      unawaited(_onSharedFilesAvailable());
    }
    _thumbnailMigrationStarted = true;
    unawaited(_mediaStore.rebuildLegacyThumbnails(
      onProgress: (generated, total) {
        if (!mounted) return;
        setState(() => _importStatus = '正在修复缩略图 $generated/$total');
      },
    ).then((_) async {
      if (!mounted) return;
      await _load();
      if (mounted && !_importing) setState(() => _importStatus = '');
    }));
  }

  void _reportImportProgress(ImportProgress progress) {
    if (!mounted) return;
    final total = progress.total;
    final submitted = progress.submitted;
    final generated = progress.thumbnailsGenerated;
    setState(() {
      if (submitted > 0) {
        _importStatus = '已提交 $submitted/$total，缩略图 $generated/$submitted';
      } else {
        _importStatus = '已准备 ${progress.prepared}/$total';
      }
    });
  }

  Future<void> _refreshAfterCommit(List<Sticker> _) async {
    if (mounted) await _load();
  }

  List<RankedSticker> get _visibleStickers => _ranking.rank(
        _stickers,
        groupId: _searchAll || _selectedGroup == 'all' ? null : _selectedGroup,
        query: _searchController.text,
        order: _sortOrder,
        filter: _filter,
      );

  /// Sticker count per group id, including `all`. Membership is many-to-many,
  /// so group counts can add up to more than the library total.
  Map<String, int> get _groupCounts {
    final counts = <String, int>{LibrarySidebar.allGroupId: _stickers.length};
    for (final entry in _stickers) {
      for (final groupId in entry.groupIds) {
        if (groupId == LibrarySidebar.allGroupId) continue;
        counts[groupId] = (counts[groupId] ?? 0) + 1;
      }
    }
    return counts;
  }

  String get _currentGroupName {
    final id = _selectedGroup ?? LibrarySidebar.allGroupId;
    if (id == LibrarySidebar.allGroupId) return LibrarySidebar.allGroupLabel;
    for (final group in _groups) {
      if (group.id == id) return group.name;
    }
    return LibrarySidebar.allGroupLabel;
  }

  Set<String> get _importGroupIds {
    final selectedGroup = _selectedGroup;
    if (_searchAll || selectedGroup == null || selectedGroup == 'all') {
      return {'all'};
    }
    return {'all', selectedGroup};
  }

  bool get _desktopImport => Platform.isWindows || Platform.isMacOS;

  Future<void> _importFiles() => _runImport(() async {
        final result = await FilePicker.pickFiles(
            dialogTitle: '选择图片或 GIF 文件',
            type: FileType.custom,
            allowedExtensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp', 'gif']);
        return [
          _ImportBatch(
              label: '已选择的文件',
              source: StickerSource.manual,
              files: result
                  .where((f) => f.path != null)
                  .map((f) => File(f.path!))
                  .toList())
        ];
      });

  Future<void> _importDirectory() => _runImport(() async {
        final directory =
            await FilePicker.getDirectoryPath(dialogTitle: '选择表情文件夹');
        if (directory == null) return [];
        final source = WindowsImportSource();
        return [
          _ImportBatch(
              label: directory,
              source: source.sourceFor(Directory(directory)),
              files: await source.scan(Directory(directory),
                  maxFiles: WindowsImportSource.defaultMaxFiles))
        ];
      });

  Future<void> _importAutoDetected() => _runImport(() async {
        final source = WindowsImportSource();
        final directories = await source.discoverCandidates(
            rememberedDirectories: await _preferences.qqImportDirectories());
        final batches = <_ImportBatch>[];
        for (final directory in directories) {
          final files = await source.scan(directory,
              maxFiles: WindowsImportSource.defaultMaxFiles);
          if (files.isNotEmpty) {
            batches.add(_ImportBatch(
                label: directory.path,
                source: source.sourceFor(directory),
                files: files));
          }
        }
        return batches;
      });

  Future<void> _runImport(Future<List<_ImportBatch>> Function() discover,
      {Future<void> Function()? cleanup}) async {
    if (_importing) {
      _showMessage('导入进行中，请稍后重试');
      await cleanup?.call();
      return;
    }
    final groups = _importGroupIds;
    setState(() {
      _importing = true;
      _importStatus = '正在准备导入预览…';
    });
    try {
      final batches = await discover();
      if (!mounted) return;
      if (batches.isEmpty || batches.every((b) => b.files.isEmpty)) {
        _showMessage('没有找到可导入的图片文件');
        return;
      }
      await _reviewAndImport(batches, groups);
    } on Object catch (error) {
      if (mounted) _showMessage('导入失败：$error');
    } finally {
      try {
        await cleanup?.call();
      } on Object {
        if (mounted) _showMessage('临时图片清理失败，请检查缓存目录权限');
      }
      if (mounted) {
        setState(() {
          _importing = false;
          _importStatus = '';
        });
      }
      _drainQueuedSharedImport();
    }
  }

  Future<bool> _reviewAndImport(
      List<_ImportBatch> batches, Set<String> groupIds) async {
    final sources = <String, StickerSource>{};
    final files = <File>[];
    for (final batch in batches) {
      for (final file in batch.files) {
        if (sources.containsKey(file.path)) continue;
        files.add(file);
        sources[file.path] = batch.source;
      }
    }
    final candidates = await _mediaStore.inspectFiles(files,
        sourceForFile: (file) => sources[file.path]!);
    if (!mounted) return false;
    setState(() => _importStatus = '请确认导入内容和目标分组');
    final target =
        groupIds.firstWhere((id) => id != 'all', orElse: () => 'all');
    final decision = await showDialog<ImportDecision>(
        context: context,
        builder: (_) => ImportPreviewDialog(
            title: '确认导入表情',
            candidates: candidates,
            groups: _groups,
            initialGroup: target));
    if (decision == null || !mounted) return false;
    final selected = decision.candidates;
    final outcome = await _mediaStore.importFiles(selected.map((c) => c.file),
        sourceForFile: (file) => sources[file.path]!,
        expectedHashes: {for (final c in selected) c.file.path: c.hash!},
        groupIds: {'all', decision.groupId},
        onProgress: _reportImportProgress,
        onRecordsCommitted: _refreshAfterCommit);
    for (final batch in batches) {
      if (batch.source == StickerSource.qq &&
          await Directory(batch.label).exists()) {
        await _preferences.rememberQqImportDirectory(batch.label);
      }
    }
    await _load();
    if (mounted) {
      _showMessage(
          '新增图片 ${outcome.added} · 已有图片加入分组 ${outcome.existingGrouped} · 已存在 ${outcome.alreadyExists} · 失败 ${outcome.skipped} · 预览跳过 ${candidates.where((c) => !c.valid).length}');
    }
    return true;
  }

  Future<void> _importClipboard() async {
    ClipboardImport? input;
    await _runImport(() async {
      input = await ClipboardBridge.instance.readForImport();
      if (input!.files.isEmpty) throw const FormatException('剪贴板中没有可导入的图片');
      return [
        _ImportBatch(
            label: '剪贴板', source: StickerSource.manual, files: input!.files)
      ];
    }, cleanup: () async {
      await input?.dispose();
    });
  }

  Future<void> _importDrop(DropDoneDetails details) async {
    final input = DropImport(details.files);
    if (ModalRoute.of(context)?.isCurrent != true) {
      if (_importing) _showMessage('导入进行中，请稍后重试');
      await input.dispose();
      return;
    }
    await _runImport(() async {
      await input.read(directorySource: WindowsImportSource());
      return [
        _ImportBatch(
            label: '拖放文件', source: StickerSource.manual, files: input.files)
      ];
    }, cleanup: input.dispose);
  }

  KeyEventResult _handleImportShortcut(FocusNode node, KeyEvent event) {
    if (!_desktopImport ||
        _quickPickerMode ||
        event is! KeyDownEvent ||
        event.logicalKey != LogicalKeyboardKey.keyV ||
        !(Platform.isMacOS
            ? HardwareKeyboard.instance.isMetaPressed
            : HardwareKeyboard.instance.isControlPressed) ||
        ModalRoute.of(context)?.isCurrent != true) {
      return KeyEventResult.ignored;
    }
    final focus = FocusManager.instance.primaryFocus?.context;
    if (focus?.widget is EditableText ||
        focus?.findAncestorStateOfType<EditableTextState>() != null) {
      return KeyEventResult.ignored;
    }
    unawaited(_importClipboard());
    return KeyEventResult.handled;
  }

  Widget _buildDropArea(Widget child) {
    if (!_desktopImport) return child;
    return DropTarget(
        onDragEntered: (_) {
          if (ModalRoute.of(context)?.isCurrent == true) {
            setState(() => _dragging = true);
          }
        },
        onDragExited: (_) => setState(() => _dragging = false),
        onDragDone: (details) {
          setState(() => _dragging = false);
          unawaited(_importDrop(details));
        },
        child: Stack(children: [
          Positioned.fill(child: child),
          if (_dragging)
            Positioned.fill(
                child: IgnorePointer(
                    child: Container(
                        color:
                            AppTheme.selectionBackground.withValues(alpha: .95),
                        alignment: Alignment.center,
                        child: Text(_importing
                            ? '导入进行中，请稍后重试'
                            : '松开以导入到「${_searchAll ? '全部表情' : _currentGroupName}」')))),
        ]));
  }

  Widget _buildImportMenu() => PopupMenuButton<String>(
      tooltip: '导入方式',
      onSelected: (value) {
        switch (value) {
          case 'clipboard':
            unawaited(_importClipboard());
          case 'directory':
            unawaited(_importDirectory());
          case 'auto':
            unawaited(_importAutoDetected());
        }
      },
      itemBuilder: (_) => [
            if (_desktopImport)
              const PopupMenuItem(value: 'clipboard', child: Text('从剪贴板导入')),
            if (_desktopImport)
              const PopupMenuItem(value: 'directory', child: Text('选择表情文件夹')),
            if (Platform.isWindows)
              const PopupMenuItem(value: 'auto', child: Text('自动扫描 QQ/微信目录')),
          ],
      icon: const Icon(Icons.arrow_drop_down));

  Future<void> _importSharedFiles({bool showEmptyMessage = false}) async {
    if (_importing) return;
    final groupIds = _importGroupIds;
    var sharedPaths = const <String>[];
    var importCompleted = false;
    setState(() {
      _importing = true;
      _importStatus = '正在接收分享文件…';
    });
    try {
      final shareErrors = await PlatformBridge.instance.consumeShareErrors();
      sharedPaths = await PlatformBridge.instance.consumeSharedFiles();
      if (sharedPaths.isEmpty) {
        if (mounted && shareErrors.isNotEmpty) {
          _showMessage(_shareErrorMessage(shareErrors));
        } else if (showEmptyMessage && mounted) {
          _showMessage('请先从 QQ/微信选择“分享”或保存图片，再返回此处接收。');
        }
        return;
      }
      final confirmed = await _reviewAndImport([
        _ImportBatch(
            label: 'Android 分享',
            source: StickerSource.androidShare,
            files: sharedPaths.map(File.new).toList())
      ], groupIds);
      if (!confirmed) return;
      await PlatformBridge.instance.acknowledgeSharedFiles(sharedPaths);
      importCompleted = true;
      if (mounted && shareErrors.isNotEmpty) {
        _showMessage(_shareErrorMessage(shareErrors));
      }
    } on Object catch (error) {
      if (mounted) _showMessage('分享导入失败：$error');
    } finally {
      if (importCompleted) {
        for (final sharedPath in sharedPaths) {
          try {
            await File(sharedPath).delete();
          } on FileSystemException {
            // Cache cleanup is best effort and must not hide an import result.
          }
        }
      }
      if (mounted) {
        setState(() {
          _importing = false;
          _importStatus = '';
        });
        _drainQueuedSharedImport();
      }
    }
  }

  String _sizeLimitSuffix(int tooLarge, int totalLimit) {
    final parts = <String>[];
    if (tooLarge > 0) parts.add('单文件过大 $tooLarge 个');
    if (totalLimit > 0) parts.add('批次大小超限 $totalLimit 个');
    return parts.isEmpty ? '' : '（${parts.join('，')}）';
  }

  String _shareErrorMessage(List<String> errors) {
    if (errors.isEmpty) return '';
    final first = errors.first.trim();
    final detail = first.isEmpty ? '' : '：$first';
    return '有 ${errors.length} 个分享文件未接收$detail';
  }

  Future<void> _onSharedFilesAvailable() async {
    if (!mounted) return;
    if (_importing) {
      _sharedImportQueued = true;
      return;
    }
    await _importSharedFiles();
  }

  void _drainQueuedSharedImport() {
    if (!mounted || !_sharedImportQueued) return;
    _sharedImportQueued = false;
    unawaited(_onSharedFilesAvailable());
  }

  void _selectGroup(String groupId) {
    _closeDrawerIfOpen();
    setState(() {
      _selectedGroup = groupId;
      _searchAll = false;
    });
    _resetKeyboardFocus();
  }

  void _closeDrawerIfOpen() {
    final scaffold = _scaffoldKey.currentState;
    if (scaffold != null && scaffold.isDrawerOpen) {
      scaffold.closeDrawer();
    }
  }

  Future<void> _createGroup() async {
    _closeDrawerIfOpen();
    final controller = TextEditingController();
    final name = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('新建分组'),
        content: TextField(
            controller: controller,
            autofocus: true,
            decoration: const InputDecoration(labelText: '分组名称')),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(context), child: const Text('取消')),
          FilledButton(
              onPressed: () => Navigator.pop(context, controller.text.trim()),
              child: const Text('创建')),
        ],
      ),
    );
    controller.dispose();
    if (name == null || name.isEmpty) return;
    final groupId = DateTime.now().microsecondsSinceEpoch.toString();
    await _repository.createGroup(groupId, name);
    if (mounted) {
      setState(() {
        _selectedGroup = groupId;
        _searchAll = false;
      });
    }
    await _load();
  }

  Future<void> _previewSticker(Sticker sticker) async {
    if (!mounted) return;
    final screenSize = MediaQuery.sizeOf(context);
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => Dialog(
        child: ConstrainedBox(
          constraints: BoxConstraints(
            maxWidth: math.min(screenSize.width * 0.9, 520),
            maxHeight: math.min(screenSize.height * 0.82, 680),
          ),
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Flexible(
                  child: Image.file(
                    File(sticker.filePath),
                    fit: BoxFit.contain,
                    gaplessPlayback: true,
                    errorBuilder: (_, __, ___) => const Center(
                      child: Icon(Icons.broken_image_outlined, size: 48),
                    ),
                  ),
                ),
                if (sticker.note.isNotEmpty) ...[
                  const SizedBox(height: 10),
                  Text(sticker.note,
                      maxLines: 2, overflow: TextOverflow.ellipsis),
                ],
                const SizedBox(height: 4),
                TextButton(
                  onPressed: () => Navigator.pop(dialogContext),
                  child: const Text('关闭'),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Future<void> _exportPackage() async {
    final controller = TextEditingController();
    final password = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('导出加密迁移包'),
        content: TextField(
          controller: controller,
          autofocus: true,
          obscureText: true,
          decoration: const InputDecoration(labelText: '密码（至少 8 个字符）'),
        ),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(context), child: const Text('取消')),
          FilledButton(
              onPressed: () => Navigator.pop(context, controller.text),
              child: const Text('导出')),
        ],
      ),
    );
    controller.dispose();
    if (password == null || password.length < 8) return;
    try {
      final bytes = await _exportService.buildPackage(password);
      final output = await FilePicker.saveFile(
        fileName: 'sticker-manager.smp',
        bytes: bytes,
        allowedExtensions: ['smp'],
        type: FileType.custom,
      );
      if (output == null) return;
      if (mounted) _showMessage('迁移包已导出');
    } on Object catch (error) {
      if (mounted) _showMessage('导出失败：$error');
    }
  }

  Future<void> _importPackage() async {
    if (_importing) {
      _showMessage('导入进行中，请稍后重试');
      return;
    }
    setState(() {
      _importing = true;
      _importStatus = '正在导入迁移包…';
    });
    try {
      final picked = await FilePicker.pickFile(
        type: FileType.custom,
        allowedExtensions: ['smp'],
      );
      final sourcePath = picked?.path;
      if (sourcePath == null) return;
      if (!mounted) return;
      final controller = TextEditingController();
      final password = await showDialog<String>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('导入加密迁移包'),
          content: TextField(
            controller: controller,
            autofocus: true,
            obscureText: true,
            decoration: const InputDecoration(labelText: '迁移包密码'),
          ),
          actions: [
            TextButton(
                onPressed: () => Navigator.pop(context),
                child: const Text('取消')),
            FilledButton(
                onPressed: () => Navigator.pop(context, controller.text),
                child: const Text('导入')),
          ],
        ),
      );
      controller.dispose();
      if (password == null) return;
      try {
        final outcome = await _exportService.importFrom(
            File(sourcePath), password, _mediaStore);
        await _load();
        if (mounted) {
          _showMessage(
              '恢复 ${outcome.added} 个，重复 ${outcome.duplicates} 个，跳过 ${outcome.skipped} 个${_sizeLimitSuffix(outcome.skippedTooLarge, outcome.skippedByTotalLimit)}');
        }
      } on Object catch (error) {
        if (mounted) _showMessage('导入失败：密码错误或文件损坏（$error）');
      }
    } finally {
      if (mounted) {
        setState(() {
          _importing = false;
          _importStatus = '';
        });
      }
      _drainQueuedSharedImport();
    }
  }

  Future<void> _togglePin(RankedSticker entry) async {
    await _repository.updateSticker(entry.sticker.copyWith(
      isPinned: !entry.sticker.isPinned,
      updatedAt: DateTime.now(),
    ));
    await _load();
  }

  Future<void> _editNote(RankedSticker entry) async {
    final controller = TextEditingController(text: entry.sticker.note);
    final note = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('编辑备注'),
        content: TextField(
            controller: controller,
            autofocus: true,
            decoration: const InputDecoration(labelText: '备注')),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(context), child: const Text('取消')),
          FilledButton(
              onPressed: () => Navigator.pop(context, controller.text.trim()),
              child: const Text('保存')),
        ],
      ),
    );
    controller.dispose();
    if (note == null) return;
    await _repository.updateSticker(
        entry.sticker.copyWith(note: note, updatedAt: DateTime.now()));
    await _load();
  }

  Future<void> _manageGroups(RankedSticker entry) async {
    final selectable = _groups.where((group) => group.id != 'all').toList();
    if (selectable.isEmpty) {
      _showMessage('请先创建一个分组');
      return;
    }
    final selected =
        entry.groupIds.where((groupId) => groupId != 'all').toSet();
    final result = await showDialog<Set<String>>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
          title: const Text('管理分组'),
          content: SizedBox(
            width: 360,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: selectable
                    .map((group) => CheckboxListTile(
                          contentPadding: EdgeInsets.zero,
                          value: selected.contains(group.id),
                          title: Text(group.name),
                          onChanged: (value) => setDialogState(() {
                            if (value == true) {
                              selected.add(group.id);
                            } else {
                              selected.remove(group.id);
                            }
                          }),
                        ))
                    .toList(),
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(dialogContext, Set.of(selected)),
              child: const Text('保存'),
            ),
          ],
        ),
      ),
    );
    if (result == null) return;
    await _repository.replaceStickerGroups(entry.sticker.id, result);
    await _load();
  }

  Future<void> _deleteSticker(RankedSticker entry) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('删除表情'),
        content: const Text('只删除表情管家中的副本，不会修改 QQ 或微信原文件。'),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(context), child: const Text('取消')),
          FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('删除')),
        ],
      ),
    );
    if (confirmed != true) return;
    if (!mounted) return;
    setState(() {
      _importing = true;
      _importStatus = '正在删除表情…';
    });
    try {
      await _mediaStore.deleteSticker(entry.sticker);
      if (mounted && _selectionMode) {
        _selectedStickerIds.remove(entry.sticker.id);
        if (_selectedStickerIds.isEmpty) {
          _exitSelectionMode();
        } else {
          setState(() {});
        }
      }
      await _load();
      if (mounted) _showMessage('已删除表情');
    } on Object catch (error) {
      if (mounted) _showMessage('删除失败：$error');
    } finally {
      if (mounted) {
        setState(() {
          _importing = false;
          _importStatus = '';
        });
        _drainQueuedSharedImport();
      }
    }
  }

  Future<void> _showStickerContextMenu(
      RankedSticker entry, Offset globalPosition) async {
    if (!isDesktopPlatform || !mounted) return;
    final overlay = Overlay.of(context).context.findRenderObject();
    if (overlay is! RenderBox) return;
    final localPosition = overlay.globalToLocal(globalPosition);
    final overlaySize = overlay.size;
    final selected = await showMenu<String>(
      context: context,
      position: RelativeRect.fromLTRB(
        localPosition.dx,
        localPosition.dy,
        overlaySize.width - localPosition.dx,
        overlaySize.height - localPosition.dy,
      ),
      items: [
        PopupMenuItem<String>(
          value: 'pin',
          child: ListTile(
            dense: true,
            contentPadding: EdgeInsets.zero,
            leading: Icon(entry.sticker.isPinned
                ? Icons.push_pin
                : Icons.push_pin_outlined),
            title: Text(entry.sticker.isPinned ? '取消置顶' : '置顶'),
          ),
        ),
        const PopupMenuItem<String>(
          value: 'edit',
          child: ListTile(
            dense: true,
            contentPadding: EdgeInsets.zero,
            leading: Icon(Icons.edit_outlined),
            title: Text('编辑备注'),
          ),
        ),
        const PopupMenuItem<String>(
          value: 'groups',
          child: ListTile(
            dense: true,
            contentPadding: EdgeInsets.zero,
            leading: Icon(Icons.folder_copy_outlined),
            title: Text('管理分组'),
          ),
        ),
        const PopupMenuDivider(),
        const PopupMenuItem<String>(
          value: 'delete',
          child: ListTile(
            dense: true,
            contentPadding: EdgeInsets.zero,
            leading: Icon(Icons.delete_outline),
            title: Text('删除表情'),
          ),
        ),
      ],
    );
    if (!mounted) return;
    switch (selected) {
      case 'pin':
        await _togglePin(entry);
      case 'edit':
        await _editNote(entry);
      case 'groups':
        await _manageGroups(entry);
      case 'delete':
        await _deleteSticker(entry);
      case null:
        break;
    }
  }

  void _enterSelectionMode([RankedSticker? entry]) {
    // The quick picker is a send-only surface. Do not let the toolbar or a
    // stale gesture put it into management selection mode.
    if (_quickPickerMode) return;
    setState(() {
      _selectionMode = true;
      if (entry != null) _selectedStickerIds.add(entry.sticker.id);
    });
    _gridFocusNode.requestFocus();
  }

  void _exitSelectionMode() {
    setState(() {
      _selectionMode = false;
      _selectedStickerIds.clear();
    });
  }

  void _toggleSelected(RankedSticker entry) {
    setState(() {
      if (!_selectionMode) _selectionMode = true;
      if (!_selectedStickerIds.add(entry.sticker.id)) {
        _selectedStickerIds.remove(entry.sticker.id);
      }
    });
  }

  void _toggleSelectAllVisible() {
    final visibleIds =
        _visibleStickers.map((entry) => entry.sticker.id).toSet();
    setState(() {
      if (visibleIds.every(_selectedStickerIds.contains)) {
        _selectedStickerIds.removeAll(visibleIds);
      } else {
        _selectedStickerIds.addAll(visibleIds);
      }
    });
  }

  Future<void> _deleteSelectedStickers() async {
    final targets = _stickers
        .where((entry) => _selectedStickerIds.contains(entry.sticker.id))
        .toList();
    if (targets.isEmpty) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('删除所选表情'),
        content: Text('将删除 ${targets.length} 个表情记录及其应用副本。QQ/微信原文件不会被修改。'),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(context), child: const Text('取消')),
          FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('确认删除')),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    setState(() {
      _importing = true;
      _importStatus = '正在删除 ${targets.length} 个表情…';
    });
    try {
      await _mediaStore.deleteStickers(targets.map((entry) => entry.sticker));
      _exitSelectionMode();
      await _load();
      if (mounted) _showMessage('已删除 ${targets.length} 个表情');
    } on Object catch (error) {
      if (mounted) _showMessage('删除失败：$error');
    } finally {
      if (mounted) {
        setState(() {
          _importing = false;
          _importStatus = '';
        });
        _drainQueuedSharedImport();
      }
    }
  }

  Future<void> _addSelectedGroups() async {
    if (_importing) return;
    final targets = _stickers
        .where((e) => _selectedStickerIds.contains(e.sticker.id))
        .toList();
    if (targets.isEmpty) return;
    final groups = await showDialog<Set<String>>(
        context: context,
        builder: (_) => AddToGroupsDialog(
            repository: _repository, entries: targets, groups: _groups));
    if (!mounted) return;
    if (groups == null) {
      await _load();
      return;
    }
    setState(() => _importing = true);
    try {
      await _repository.attachGroupsMany(
          targets.map((e) => e.sticker.id), groups);
      await _load();
      if (mounted) {
        _showMessage('已将 ${targets.length} 张表情添加到 ${groups.length} 个分组');
      }
    } on Object catch (error) {
      if (mounted) _showMessage('添加分组失败：$error');
    } finally {
      if (mounted) setState(() => _importing = false);
      _drainQueuedSharedImport();
    }
  }

  Future<void> _manageSelectedGroups() async {
    if (_importing) return;
    final targets = _stickers
        .where((entry) => _selectedStickerIds.contains(entry.sticker.id))
        .toList(growable: false);
    if (targets.isEmpty) return;
    final selectable = _groups.where((group) => group.id != 'all').toList();
    if (selectable.isEmpty) {
      _showMessage('请先创建一个分组');
      return;
    }
    final result = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text('批量移动 ${targets.length} 个表情'),
        content: SizedBox(
          width: 360,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('点击目标分组后立即移动。原有用户分组会被替换，“全部”始终保留。'),
                const SizedBox(height: 8),
                ...selectable.map((group) => CheckboxListTile(
                      contentPadding: EdgeInsets.zero,
                      value: false,
                      title: Text(group.name),
                      onChanged: (value) {
                        if (value == true) {
                          Navigator.pop(dialogContext, group.id);
                        }
                      },
                    )),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text('取消'),
          ),
        ],
      ),
    );
    if (result == null || !mounted) return;
    _exitSelectionMode();
    setState(() {
      _importing = true;
      _importStatus = '正在移动 ${targets.length} 个表情…';
    });
    try {
      await _repository.replaceStickerGroupsMany(
          targets.map((entry) => entry.sticker.id), [result]);
      if (!mounted) return;
      await _load();
      if (mounted) _showMessage('已移动 ${targets.length} 个表情');
    } on Object catch (error) {
      if (mounted) _showMessage('批量移动失败：$error');
    } finally {
      if (mounted) {
        setState(() {
          _importing = false;
          _importStatus = '';
        });
        _drainQueuedSharedImport();
      }
    }
  }

  String _sourceLabel(StickerSource source) {
    switch (source) {
      case StickerSource.qq:
        return 'QQ 导入';
      case StickerSource.wechat:
        return '微信导入';
      case StickerSource.manual:
        return '手动导入';
      case StickerSource.androidShare:
        return 'Android 分享';
    }
  }

  Future<void> _cleanupBySource() async {
    final counts = <StickerSource, int>{
      for (final source in StickerSource.values) source: 0,
    };
    for (final entry in _stickers) {
      counts[entry.sticker.source] = counts[entry.sticker.source]! + 1;
    }
    final available =
        StickerSource.values.where((source) => counts[source]! > 0).toList();
    if (available.isEmpty) {
      _showMessage('没有可清理的导入记录');
      return;
    }

    final selected = <StickerSource>{};
    final sources = await showDialog<Set<StickerSource>>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
          title: const Text('按来源清理'),
          content: SizedBox(
            width: 360,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: available
                  .map((source) => CheckboxListTile(
                        contentPadding: EdgeInsets.zero,
                        value: selected.contains(source),
                        title: Text(_sourceLabel(source)),
                        subtitle: Text('${counts[source]} 个记录'),
                        onChanged: (value) => setDialogState(() {
                          if (value == true) {
                            selected.add(source);
                          } else {
                            selected.remove(source);
                          }
                        }),
                      ))
                  .toList(),
            ),
          ),
          actions: [
            TextButton(
                onPressed: () => Navigator.pop(dialogContext),
                child: const Text('取消')),
            FilledButton(
              onPressed: selected.isEmpty
                  ? null
                  : () => Navigator.pop(dialogContext, Set.of(selected)),
              child: const Text('下一步'),
            ),
          ],
        ),
      ),
    );
    if (sources == null || sources.isEmpty) return;

    final targets = _stickers
        .where((entry) => sources.contains(entry.sticker.source))
        .toList();
    if (!mounted) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('确认批量删除'),
        content: Text('将删除 ${targets.length} 个表情记录及其应用副本。QQ/微信原文件不会被修改。'),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(context), child: const Text('取消')),
          FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('确认删除')),
        ],
      ),
    );
    if (confirmed != true) return;
    if (!mounted) return;
    setState(() {
      _importing = true;
      _importStatus = '正在删除 ${targets.length} 个表情…';
    });
    try {
      await _mediaStore.deleteStickers(targets.map((entry) => entry.sticker));
      await _load();
      if (mounted) _showMessage('已删除 ${targets.length} 个表情记录');
    } on Object catch (error) {
      if (mounted) _showMessage('删除失败：$error');
    } finally {
      if (mounted) {
        setState(() {
          _importing = false;
          _importStatus = '';
        });
        _drainQueuedSharedImport();
      }
    }
  }

  Future<void> _useSticker(RankedSticker entry) async {
    if (_stickerActionBusy ||
        (_lastStickerActionId == entry.sticker.id &&
            _lastStickerAction != null &&
            DateTime.now().difference(_lastStickerAction!) <
                const Duration(milliseconds: 350))) {
      return;
    }
    _stickerActionBusy = true;
    try {
      final result = await PlatformBridge.instance.useSticker(entry.sticker);
      if (result.succeeded) await _recordRecent(entry.sticker.id);
      await _recordCompatibility(entry.sticker, result);
      final countsAsUsage = result.succeeded &&
          (result.didPaste ||
              ((Platform.isAndroid || Platform.isMacOS) &&
                  result.status == StickerUseStatus.copied));
      if (countsAsUsage) {
        await _repository.recordUsage(entry.sticker.id, DateTime.now());
        await _load();
        if (mounted) {
          final message = result.message ??
              (result.status == StickerUseStatus.copied
                  ? Platform.isWindows
                      ? '已复制 ${path.basename(entry.sticker.filePath)}，当前没有发送目标窗口'
                      : Platform.isMacOS
                          ? '已复制，请切换到目标应用按 ⌘V 粘贴'
                          : '已复制到系统剪贴板'
                  : result.status == StickerUseStatus.sent
                      ? '已发送到 ${result.targetApplication ?? '目标窗口'}'
                      : null);
          if (message != null) _showMessage(message);
        }
      } else if (result.succeeded) {
        if (mounted) {
          _showMessage(Platform.isWindows
              ? '已复制 ${path.basename(entry.sticker.filePath)}，当前没有发送目标窗口'
              : Platform.isMacOS
                  ? '已复制，请切换到目标应用按 ⌘V 粘贴'
                  : '已复制到系统剪贴板');
        }
      } else if (mounted) {
        _showMessage(result.message ?? '复制或发送失败');
      }
    } finally {
      _stickerActionBusy = false;
      _lastStickerAction = DateTime.now();
      _lastStickerActionId = entry.sticker.id;
    }
  }

  /// Card copy button: clipboard only, no window activation, paste or Enter.
  /// Usage counting follows the platform rule: a Windows copy never counts,
  /// while on macOS and Android copying is itself a successful use.
  Future<void> _copySticker(RankedSticker entry) async {
    if (_stickerActionBusy ||
        (_lastStickerActionId == entry.sticker.id &&
            _lastStickerAction != null &&
            DateTime.now().difference(_lastStickerAction!) <
                const Duration(milliseconds: 350))) {
      return;
    }
    _stickerActionBusy = true;
    try {
      final result = await PlatformBridge.instance.copySticker(entry.sticker);
      if (result.succeeded) await _recordRecent(entry.sticker.id);
      if (result.succeeded && (Platform.isMacOS || Platform.isAndroid)) {
        await _repository.recordUsage(entry.sticker.id, DateTime.now());
        await _load();
      }
      if (!mounted) return;
      _showMessage(result.succeeded ? '已复制' : (result.message ?? '复制失败'));
    } finally {
      _stickerActionBusy = false;
      _lastStickerAction = DateTime.now();
      _lastStickerActionId = entry.sticker.id;
    }
  }

  Future<void> _recordRecent(String id) async {
    await _recentUsage.record(id);
    final ids = await _recentUsage.ids();
    if (mounted) setState(() => _recentIds = ids);
  }

  Widget _buildRecent() {
    if (!_showRecent ||
        _selectionMode ||
        _filter.activeCount > 0 ||
        _searchController.text.trim().isNotEmpty) {
      return const SizedBox.shrink();
    }
    final byId = {for (final entry in _stickers) entry.sticker.id: entry};
    final entries = [
      for (final id in _recentIds)
        if (byId[id] != null) byId[id]!
    ];
    if (entries.isEmpty) return const SizedBox.shrink();
    return RecentStickers(
        entries: entries,
        onUse: (e) => unawaited(_useSticker(e)),
        onCopy: (e) => unawaited(_copySticker(e)));
  }

  Future<void> _recordCompatibility(
      Sticker sticker, StickerUseResult result) async {
    if (!Platform.isWindows || result.targetApplication == null) return;
    await _preferences.recordCompatibility(ClipboardCompatibilityRecord(
      targetApplication: result.targetApplication!,
      mediaType: sticker.mediaType,
      status: enumValue(result.status),
      createdAt: DateTime.now(),
      message: result.message,
    ));
  }

  Future<void> _consumeFloatingUsage() async {
    if (!Platform.isAndroid || _floatingUsageSyncInProgress) return;
    _floatingUsageSyncInProgress = true;
    try {
      final ids = await PlatformBridge.instance.peekFloatingUsage();
      if (ids.isEmpty) return;
      await _repository.recordUsageMany(ids, DateTime.now());
      for (final id in ids) {
        await _recentUsage.record(id);
      }
      await PlatformBridge.instance.acknowledgeFloatingUsage(ids);
      await _load();
    } finally {
      _floatingUsageSyncInProgress = false;
    }
  }

  void _showMessage(String message) {
    if (!mounted) return;
    _feedbackTimer?.cancel();
    setState(() => _feedbackMessage = message);
    _feedbackTimer = Timer(const Duration(seconds: 3), () {
      if (mounted) setState(() => _feedbackMessage = null);
    });
  }

  Future<void> _refreshFloatingPanelState() async {
    final enabled = await PlatformBridge.instance.isFloatingPanelRunning();
    if (mounted) setState(() => _floatingPanelEnabled = enabled);
  }

  Future<void> _toggleFloatingPanel() async {
    if (!Platform.isAndroid) return;
    if (_floatingPanelEnabled) {
      await PlatformBridge.instance.stopFloatingPanel();
      if (mounted) {
        setState(() => _floatingPanelEnabled = false);
        _showMessage('悬浮面板已关闭');
      }
      return;
    }
    final started = await PlatformBridge.instance.startFloatingPanel(
        _ranking.rank(_stickers).map((entry) => entry.sticker));
    if (!mounted) return;
    if (started) {
      setState(() => _floatingPanelEnabled = true);
      _showMessage('悬浮面板已开启');
    } else {
      _showMessage('请在系统设置中允许“表情管家”显示在其他应用上层，然后重试');
    }
  }

  Future<void> _setDensity(GridDensity density) async {
    if (density == _density) return;
    setState(() => _density = density);
    await _preferences.setGridDensity(density);
  }

  Future<void> _handleMoreMenu(String value) async {
    switch (value) {
      case 'quick_picker':
        await PlatformBridge.instance.showQuickPicker();
      case 'export':
        await _exportPackage();
      case 'import':
        await _importPackage();
      case 'cleanup':
        await _cleanupBySource();
      case 'compatibility':
        await _showCompatibilityRecords();
      case 'hotkey':
        await _configureHotKey();
    }
  }

  Future<void> _showSettings() async {
    _closeDrawerIfOpen();
    final records = Platform.isWindows
        ? await _preferences.compatibilityRecords()
        : <ClipboardCompatibilityRecord>[];
    if (!mounted) return;
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
          title: const Text('设置'),
          scrollable: true,
          actionsPadding: const EdgeInsets.fromLTRB(24, 0, 24, 24),
          content: SizedBox(
            width: 480,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SwitchListTile(
                  contentPadding: const EdgeInsets.only(left: 12),
                  title: const Text('显示最近使用'),
                  value: _showRecent,
                  onChanged: (value) async {
                    await _preferences.setShowRecent(value);
                    if (!mounted) return;
                    setState(() => _showRecent = value);
                    if (dialogContext.mounted) setDialogState(() {});
                  },
                ),
                if (isDesktopPlatform)
                  HotkeySetting(
                    current: QuickPickerController.instance.hotKey,
                    onSave: (value) async {
                      final saved = await QuickPickerController.instance
                          .updateHotKey(value);
                      if (dialogContext.mounted) setDialogState(() {});
                      return saved;
                    },
                  ),
                if (Platform.isWindows) ...[
                  const Padding(
                    padding: EdgeInsets.all(12),
                    child: Text('剪贴板兼容性记录'),
                  ),
                  if (records.isEmpty)
                    const Padding(
                        padding: EdgeInsets.symmetric(horizontal: 12),
                        child: Text('还没有目标应用发送记录')),
                  for (final record in records)
                    ListTile(
                      dense: true,
                      title: Text(record.targetApplication),
                      subtitle: Text(
                          '${enumValue(record.mediaType).toUpperCase()} · ${_compatibilityDetails(record)}'),
                      trailing:
                          Text(_formatCompatibilityTime(record.createdAt)),
                    ),
                ],
                if (Platform.isAndroid)
                  SwitchListTile(
                    contentPadding: const EdgeInsets.only(left: 12),
                    title: const Text('悬浮面板'),
                    value: _floatingPanelEnabled,
                    onChanged: (_) async {
                      await _toggleFloatingPanel();
                      if (dialogContext.mounted) setDialogState(() {});
                    },
                  ),
              ],
            ),
          ),
          actions: [
            FilledButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: const Text('关闭'),
            ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final visible = _visibleStickers;
    return _quickPickerMode
        ? _buildQuickPickerScaffold(visible)
        : Focus(
            onKeyEvent: _handleImportShortcut,
            child: _buildManagementScaffold(visible));
  }

  Widget _buildManagementScaffold(List<RankedSticker> visible) {
    final searchQuery = _searchController.text.trim();
    return LayoutBuilder(
      builder: (context, constraints) {
        final wide = constraints.maxWidth >= 1000;
        final sidebar = LibrarySidebar(
          groups: _groups,
          groupCounts: _groupCounts,
          selectedGroupId: _selectedGroup ?? LibrarySidebar.allGroupId,
          onSelectGroup: _selectGroup,
          onCreateGroup: _createGroup,
          onOpenSettings: _showSettings,
        );
        return Scaffold(
          key: _scaffoldKey,
          drawer: wide ? null : Drawer(child: SafeArea(child: sidebar)),
          body: SafeArea(
            child: Stack(
              children: [
                Row(
                  children: [
                    if (wide) ...[
                      sidebar,
                      const VerticalDivider(width: 1, color: AppTheme.border),
                    ],
                    Expanded(
                      child: Column(
                        children: [
                          if (_selectionMode)
                            SelectionToolbar(
                              selectedCount: _selectedStickerIds.length,
                              onSelectAll: _toggleSelectAllVisible,
                              onAdd: _selectedStickerIds.isEmpty
                                  ? null
                                  : _addSelectedGroups,
                              onMove: _selectedStickerIds.isEmpty
                                  ? null
                                  : _manageSelectedGroups,
                              onDelete: _selectedStickerIds.isEmpty
                                  ? null
                                  : _deleteSelectedStickers,
                              onExit: _exitSelectionMode,
                            )
                          else
                            LibraryToolbar(
                              searchAll: _searchAll,
                              onScopeChanged: _changeSearchScope,
                              onFilter: _showFilters,
                              filterCount: _filter.activeCount,
                              searchController: _searchController,
                              onSearchChanged: (_) =>
                                  setState(_resetKeyboardFocus),
                              onClearSearch: _clearSearch,
                              onEnterSelection: _enterSelectionMode,
                              onImport: _importFiles,
                              importMenu:
                                  _desktopImport ? _buildImportMenu() : null,
                              moreMenu: _buildMoreMenu(quickPicker: false),
                              fixedControlHeight: isDesktopPlatform,
                              leading: wide
                                  ? null
                                  : IconButton(
                                      tooltip: '分组导航',
                                      onPressed: () => _scaffoldKey.currentState
                                          ?.openDrawer(),
                                      icon: const Icon(Icons.menu),
                                    ),
                            ),
                          Expanded(
                              child: _buildDropArea(
                                  _buildContentArea(visible, searchQuery))),
                        ],
                      ),
                    ),
                  ],
                ),
                LibraryFeedback(
                  message: _feedbackMessage,
                  progressActive: _importing,
                  progressText: _importStatus,
                ),
              ],
            ),
          ),
          floatingActionButton: Platform.isAndroid
              ? FloatingActionButton.extended(
                  onPressed: () => _importSharedFiles(showEmptyMessage: true),
                  icon: const Icon(Icons.share_outlined),
                  label: const Text('接收分享'),
                )
              : null,
        );
      },
    );
  }

  void _clearSearch() {
    _searchController.clear();
    setState(_resetKeyboardFocus);
  }

  void _changeSearchScope(bool all) {
    setState(() {
      _searchAll = all;
      _resetKeyboardFocus();
    });
  }

  void _clearFilters() {
    setState(() {
      _filter = const StickerFilter();
      _resetKeyboardFocus();
    });
  }

  Future<void> _showFilters() async {
    final platform = Theme.of(context).platform;
    final mobile =
        platform == TargetPlatform.android || platform == TargetPlatform.iOS;
    final StickerFilter? result;
    if (mobile) {
      result = await showModalBottomSheet<StickerFilter>(
        context: context,
        isScrollControlled: true,
        useSafeArea: true,
        builder: (context) => ConstrainedBox(
          constraints: BoxConstraints(
            maxHeight: MediaQuery.sizeOf(context).height * .9,
          ),
          child: FilterPanel(initial: _filter),
        ),
      );
    } else {
      result = await showDialog<StickerFilter>(
        context: context,
        builder: (context) => Dialog(
          alignment: Alignment.topRight,
          child: SizedBox(width: 420, child: FilterPanel(initial: _filter)),
        ),
      );
    }
    final applied = result;
    if (!mounted || applied == null) return;
    setState(() {
      _filter = applied;
      _resetKeyboardFocus();
    });
  }

  Widget _buildFilterSummary() {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Row(
        children: [
          Expanded(
            child: Tooltip(
              message: _filter.summary,
              child: Text(
                _filter.summary,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ),
          TextButton(onPressed: _clearFilters, child: const Text('清除筛选')),
        ],
      ),
    );
  }

  Widget _buildEmptyResults() => _SearchEmptyState(
        onClearSearch:
            _searchController.text.trim().isEmpty ? null : _clearSearch,
        onClearFilters: _filter.activeCount == 0 ? null : _clearFilters,
        onSearchAll:
            _searchAll || _selectedGroup == null || _selectedGroup == 'all'
                ? null
                : () => _changeSearchScope(true),
      );

  Widget _buildContentArea(List<RankedSticker> visible, String searchQuery) {
    if (_loading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_error != null) return _buildLoadError();
    return Column(
      children: [
        if (_filter.activeCount > 0) _buildFilterSummary(),
        _buildRecent(),
        _buildContentHeader(visible, searchQuery),
        Expanded(
          child: visible.isEmpty
              ? (searchQuery.isNotEmpty || _filter.activeCount > 0
                  ? _buildEmptyResults()
                  : _EmptyState(onImport: _importFiles))
              : _buildGrid(visible),
        ),
      ],
    );
  }

  Widget _buildLoadError() {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.error_outline,
                size: 48, color: AppTheme.secondaryText),
            const SizedBox(height: 12),
            Text('初始化失败：$_error'),
            const SizedBox(height: 16),
            FilledButton(onPressed: _load, child: const Text('重试')),
          ],
        ),
      ),
    );
  }

  Widget _buildContentHeader(List<RankedSticker> visible, String searchQuery) {
    final countLabel = searchQuery.isEmpty && _filter.activeCount == 0
        ? '${visible.length} 张表情'
        : '搜索到 ${visible.length} 个表情';
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        AppTheme.contentPadding,
        20,
        AppTheme.contentPadding,
        0,
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final narrow = constraints.maxWidth < 550;
          final title = Row(
            crossAxisAlignment: CrossAxisAlignment.baseline,
            textBaseline: TextBaseline.alphabetic,
            children: [
              Flexible(
                child: Text(
                  _searchAll ? '全部表情' : _currentGroupName,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontSize: 24,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              const SizedBox(width: 24),
              Text(
                countLabel,
                style: const TextStyle(
                  fontSize: 14,
                  color: AppTheme.secondaryText,
                ),
              ),
            ],
          );
          final controls = Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              _buildSortControl(),
              const SizedBox(width: 16),
              _buildDensityControl(),
            ],
          );
          if (narrow) {
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                title,
                const SizedBox(height: 16),
                Align(alignment: Alignment.centerRight, child: controls),
              ],
            );
          }
          return Row(
            children: [
              Expanded(child: title),
              const SizedBox(width: 24),
              controls,
            ],
          );
        },
      ),
    );
  }

  Widget _buildSortControl() {
    final defaultLabel =
        !_searchAll && _selectedGroup == 'qq_favorites' ? '来源顺序' : '常用优先';
    return Container(
      width: 148,
      height: AppTheme.secondaryControlHeight,
      padding: const EdgeInsets.symmetric(horizontal: 18),
      decoration: BoxDecoration(
        color: AppTheme.cardBackground,
        border: Border.all(color: AppTheme.border),
        borderRadius: BorderRadius.circular(AppTheme.buttonRadius),
      ),
      child: DropdownButtonHideUnderline(
        child: DropdownButton<StickerSortOrder>(
          value: _sortOrder,
          isDense: true,
          isExpanded: true,
          borderRadius: BorderRadius.circular(AppTheme.buttonRadius),
          icon: const Icon(Icons.keyboard_arrow_down_rounded,
              size: 22, color: AppTheme.primaryText),
          style: Theme.of(context).textTheme.bodyLarge!.copyWith(
                fontSize: 16,
                color: AppTheme.primaryText,
              ),
          items: [
            DropdownMenuItem(
              value: StickerSortOrder.defaultRule,
              child: Text(defaultLabel),
            ),
            const DropdownMenuItem(
              value: StickerSortOrder.recentImport,
              child: Text('最近导入'),
            ),
          ],
          onChanged: (value) {
            if (value == null) return;
            setState(() {
              _sortOrder = value;
              _restoreKeyboardFocus();
            });
            _scrollToFocusedSticker(_focusedStickerIndex);
          },
        ),
      ),
    );
  }

  Widget _buildDensityControl() {
    return PopupMenuButton<GridDensity>(
      tooltip: '缩略图密度',
      initialValue: _density,
      onSelected: (density) => unawaited(_setDensity(density)),
      itemBuilder: (context) => [
        CheckedPopupMenuItem(
          value: GridDensity.standard,
          checked: _density == GridDensity.standard,
          child: const Text('标准'),
        ),
        CheckedPopupMenuItem(
          value: GridDensity.compact,
          checked: _density == GridDensity.compact,
          child: const Text('紧凑'),
        ),
      ],
      child: Container(
        width: AppTheme.secondaryControlHeight,
        height: AppTheme.secondaryControlHeight,
        decoration: BoxDecoration(
          color: AppTheme.cardBackground,
          border: Border.all(color: AppTheme.border),
          borderRadius: BorderRadius.circular(AppTheme.buttonRadius),
        ),
        child: const Icon(Icons.grid_view_outlined,
            size: 24, color: AppTheme.primaryText),
      ),
    );
  }

  Widget _buildMoreMenu({required bool quickPicker}) {
    return PopupMenuButton<String>(
      tooltip: '更多操作',
      onSelected: (value) => unawaited(_handleMoreMenu(value)),
      itemBuilder: (context) => [
        if (isDesktopPlatform && !quickPicker)
          const PopupMenuItem(value: 'quick_picker', child: Text('快速唤出')),
        if (isDesktopPlatform && quickPicker)
          const PopupMenuItem(value: 'hotkey', child: Text('设置快速唤出热键')),
        if (Platform.isWindows && quickPicker)
          const PopupMenuItem(value: 'compatibility', child: Text('剪贴板兼容性记录')),
        const PopupMenuItem(value: 'export', child: Text('导出加密迁移包')),
        const PopupMenuItem(value: 'import', child: Text('导入加密迁移包')),
        const PopupMenuItem(value: 'cleanup', child: Text('清理导入记录')),
      ],
      child: Container(
        width: quickPicker ? 38 : AppTheme.controlHeight,
        height: quickPicker ? 38 : AppTheme.controlHeight,
        decoration: BoxDecoration(
          color: AppTheme.cardBackground,
          border: Border.all(color: AppTheme.border),
          borderRadius: BorderRadius.circular(AppTheme.buttonRadius),
        ),
        child:
            const Icon(Icons.more_horiz, size: 26, color: AppTheme.primaryText),
      ),
    );
  }

  Widget _buildGrid(List<RankedSticker> visible) {
    return Listener(
      onPointerDown: (_) => _gridFocusNode.requestFocus(),
      child: KeyboardListener(
        focusNode: _gridFocusNode,
        onKeyEvent: _handleGridKey,
        child: ListenableBuilder(
          listenable: _gridFocusNode,
          builder: (context, child) => StickerGrid(
            stickers: visible,
            density: _density,
            quickPicker: _quickPickerMode,
            selectionMode: _selectionMode,
            selectedIds: _selectedStickerIds,
            focusedIndex: _focusedStickerIndex,
            showKeyboardFocus: _gridFocusNode.hasPrimaryFocus,
            scrollController: _gridScrollController,
            onMetricsChanged: _handleGridMetricsChanged,
            onUse: (entry) => unawaited(_useSticker(entry)),
            onSelect: _toggleSelected,
            onDragSelection: (ids) => setState(() {
              _selectedStickerIds
                ..clear()
                ..addAll(ids);
            }),
            onExitSelection: _exitSelectionMode,
            onCopy: (entry) => unawaited(_copySticker(entry)),
            onLongPress: Platform.isAndroid
                ? (entry) => unawaited(_previewSticker(entry.sticker))
                : _quickPickerMode
                    ? null
                    : _enterSelectionMode,
            onPin: (entry) => unawaited(_togglePin(entry)),
            onGroups: (entry) => unawaited(_manageGroups(entry)),
            onEdit: (entry) => unawaited(_editNote(entry)),
            onDelete: (entry) => unawaited(_deleteSticker(entry)),
            onContextMenu: _showStickerContextMenu,
          ),
        ),
      ),
    );
  }

  Widget _buildQuickPickerScaffold(List<RankedSticker> visible) {
    final searchQuery = _searchController.text.trim();
    return Scaffold(
      body: Stack(
        children: [
          Column(
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(14, 6, 6, 0),
                child: Row(
                  children: [
                    const Text(
                      '快速选择',
                      style:
                          TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
                    ),
                    const Spacer(),
                    IconButton(
                      tooltip: '导入表情',
                      onPressed: _importFiles,
                      icon: const Icon(Icons.file_upload_outlined, size: 20),
                    ),
                    IconButton(
                      tooltip: '快速唤出',
                      onPressed: PlatformBridge.instance.showQuickPicker,
                      icon: const Icon(Icons.flash_on_outlined, size: 20),
                    ),
                    _buildMoreMenu(quickPicker: true),
                  ],
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(10, 0, 10, 6),
                child: TextField(
                  controller: _searchController,
                  onChanged: (_) => setState(_resetKeyboardFocus),
                  decoration: InputDecoration(
                    hintText: '搜索备注或表情 ID',
                    prefixIcon: const Icon(Icons.search,
                        size: 20, color: AppTheme.secondaryText),
                    prefixIconConstraints:
                        const BoxConstraints(minWidth: 40, minHeight: 36),
                    suffixIcon: _searchController.text.isEmpty
                        ? null
                        : IconButton(
                            onPressed: _clearSearch,
                            tooltip: '清除搜索',
                            icon: const Icon(Icons.clear, size: 18),
                          ),
                  ),
                ),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 10),
                child: Row(
                  children: [
                    DropdownButton<bool>(
                      value: _searchAll,
                      onChanged: (value) {
                        if (value != null) _changeSearchScope(value);
                      },
                      items: const [
                        DropdownMenuItem(value: false, child: Text('当前分组')),
                        DropdownMenuItem(value: true, child: Text('全部表情')),
                      ],
                    ),
                    const Spacer(),
                    TextButton.icon(
                      onPressed: _showFilters,
                      icon: const Icon(Icons.filter_list),
                      label: Text(
                        _filter.activeCount == 0
                            ? '筛选'
                            : '筛选 · ${_filter.activeCount}',
                      ),
                    ),
                  ],
                ),
              ),
              _buildRecent(),
              if (_filter.activeCount > 0) _buildFilterSummary(),
              SizedBox(
                height: 40,
                child: ListView(
                  padding: const EdgeInsets.symmetric(horizontal: 10),
                  scrollDirection: Axis.horizontal,
                  children: [
                    ..._groups.map((group) => Padding(
                          padding: const EdgeInsets.only(right: 8),
                          child: ChoiceChip(
                            label: Text(group.name),
                            selected: (_selectedGroup ?? 'all') == group.id,
                            onSelected: (_) => _selectGroup(group.id),
                          ),
                        )),
                    ActionChip(
                      avatar: const Icon(Icons.add, size: 18),
                      label: const Text('新分组'),
                      onPressed: _createGroup,
                    ),
                  ],
                ),
              ),
              Expanded(
                child: _loading
                    ? const Center(child: CircularProgressIndicator())
                    : _error != null
                        ? _buildLoadError()
                        : visible.isEmpty
                            ? (searchQuery.isNotEmpty || _filter.activeCount > 0
                                ? _buildEmptyResults()
                                : _EmptyState(onImport: _importFiles))
                            : _buildGrid(visible),
              ),
            ],
          ),
          LibraryFeedback(
            message: _feedbackMessage,
            progressActive: _importing,
            progressText: _importStatus,
          ),
        ],
      ),
    );
  }

  Future<void> _showCompatibilityRecords() async {
    if (!Platform.isWindows) return;
    final records = await _preferences.compatibilityRecords();
    if (!mounted) return;
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('剪贴板兼容性记录'),
        content: SizedBox(
          width: 460,
          height: 360,
          child: records.isEmpty
              ? const Center(child: Text('还没有目标应用发送记录'))
              : ListView.separated(
                  itemCount: records.length,
                  separatorBuilder: (_, __) => const Divider(height: 1),
                  itemBuilder: (_, index) {
                    final record = records[index];
                    final status = switch (record.status) {
                      'sent' => '发送成功',
                      'copied' => '已粘贴',
                      _ => '失败',
                    };
                    final details =
                        record.message == null || record.message!.trim().isEmpty
                            ? status
                            : '$status：${record.message}';
                    return ListTile(
                      dense: true,
                      title: Text(record.targetApplication),
                      subtitle: Text(
                          '${enumValue(record.mediaType).toUpperCase()} · $details'),
                      trailing: Text(
                        _formatCompatibilityTime(record.createdAt),
                        style: Theme.of(dialogContext).textTheme.bodySmall,
                      ),
                    );
                  },
                ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text('关闭'),
          ),
        ],
      ),
    );
  }

  String _compatibilityDetails(ClipboardCompatibilityRecord record) {
    final status = switch (record.status) {
      'sent' => '发送成功',
      'copied' => '已粘贴',
      _ => '失败',
    };
    return record.message == null || record.message!.trim().isEmpty
        ? status
        : '$status：${record.message}';
  }

  String _formatCompatibilityTime(DateTime value) {
    final local = value.toLocal();
    String twoDigits(int number) => number.toString().padLeft(2, '0');
    return '${local.month}/${local.day} ${twoDigits(local.hour)}:${twoDigits(local.minute)}';
  }

  Future<void> _configureHotKey() => _showSettings();
}

class _ImportBatch {
  const _ImportBatch({
    required this.label,
    required this.source,
    required this.files,
  });

  final String label;
  final StickerSource source;
  final List<File> files;

  /// QQ's received and marketplace folders are not part of the user's
  /// personal collection. Keep them visible for review, but leave them
  /// unchecked so a broad scan cannot silently add chat history.
  bool get isReceivedOrMarketPath {
    return isReceivedOrMarketImportPath(label);
  }

  bool get defaultSelected => defaultSelectImportBatch(label);
}

class _EmptyState extends StatelessWidget {
  const _EmptyState({required this.onImport});

  final VoidCallback onImport;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Icon(Icons.collections_outlined,
              size: 64, color: AppTheme.accent),
          const SizedBox(height: 16),
          const Text('还没有表情包',
              style: TextStyle(fontSize: 20, fontWeight: FontWeight.w600)),
          const SizedBox(height: 8),
          const Text('从文件选择图片或 GIF，开始建立你的表情库'),
          const SizedBox(height: 20),
          FilledButton.icon(
              onPressed: onImport,
              icon: const Icon(Icons.file_upload_outlined),
              label: const Text('导入表情')),
        ],
      ),
    );
  }
}

class _SearchEmptyState extends StatelessWidget {
  const _SearchEmptyState({
    this.onClearSearch,
    this.onClearFilters,
    this.onSearchAll,
  });

  final VoidCallback? onClearSearch;
  final VoidCallback? onClearFilters;
  final VoidCallback? onSearchAll;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(12),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.search_off,
                size: 56, color: AppTheme.secondaryText),
            const SizedBox(height: 12),
            const Text('没有匹配的表情',
                style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600)),
            const SizedBox(height: 6),
            const Text('可以调整关键词、筛选条件或搜索范围'),
            const SizedBox(height: 16),
            Wrap(
              alignment: WrapAlignment.center,
              children: [
                if (onClearSearch != null)
                  TextButton(
                      onPressed: onClearSearch, child: const Text('清除搜索')),
                if (onClearFilters != null)
                  TextButton(
                    onPressed: onClearFilters,
                    child: const Text('清除筛选'),
                  ),
                if (onSearchAll != null)
                  TextButton(
                      onPressed: onSearchAll, child: const Text('搜索全部表情')),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
