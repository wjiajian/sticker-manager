import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:hotkey_manager/hotkey_manager.dart';
import 'package:path/path.dart' as path;

import '../models.dart';
import '../platform/desktop_platform.dart';
import '../platform/platform_bridge.dart';
import '../services/database.dart';
import '../services/export_service.dart';
import '../services/import_source.dart';
import '../services/media_store.dart';
import '../services/preferences.dart';
import '../services/quick_picker_controller.dart';
import '../services/ranking_service.dart';
import '../services/repository.dart';
import 'app_theme.dart';
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
  final _searchController = TextEditingController();
  final _gridFocusNode = FocusNode(debugLabel: 'sticker-grid');
  final _gridScrollController = ScrollController();
  final _scaffoldKey = GlobalKey<ScaffoldState>();
  List<RankedSticker> _stickers = [];
  List<StickerGroup> _groups = [];
  String? _selectedGroup;
  bool _loading = true;
  bool _importing = false;
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
      setState(() {
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
        groupId: _selectedGroup == 'all' ? null : _selectedGroup,
        query: _searchController.text,
        order: _sortOrder,
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
    if (selectedGroup == null || selectedGroup == 'all') {
      return {'all'};
    }
    return {'all', selectedGroup};
  }

  Future<void> _importFiles() async {
    if (_importing) return;
    final groupIds = _importGroupIds;
    if (Platform.isWindows) {
      final mode = await _showWindowsImportOptions();
      if (!mounted || mode == null) return;
      if (mode == 'auto') {
        await _importAutoDetected(groupIds);
      } else if (mode == 'directory') {
        await _importDirectory(groupIds);
      } else {
        await _importSelectedFiles(groupIds);
      }
      return;
    }
    await _importSelectedFiles(groupIds);
  }

  Future<String?> _showWindowsImportOptions() async {
    return showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('导入表情'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.manage_search_outlined),
              title: const Text('自动扫描 QQ/微信目录'),
              subtitle: const Text('查找已知的个人表情目录'),
              onTap: () => Navigator.pop(context, 'auto'),
            ),
            ListTile(
              leading: const Icon(Icons.folder_open_outlined),
              title: const Text('选择表情文件夹'),
              subtitle: const Text('直接选择 QQ 的 Ori 文件夹或其他表情目录'),
              onTap: () => Navigator.pop(context, 'directory'),
            ),
            ListTile(
              leading: const Icon(Icons.photo_library_outlined),
              title: const Text('选择图片或 GIF 文件'),
              subtitle: const Text('可一次选择多个文件'),
              onTap: () => Navigator.pop(context, 'files'),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('取消'),
          ),
        ],
      ),
    );
  }

  Future<void> _importAutoDetected(Iterable<String> groupIds) async {
    if (_importing) return;
    setState(() {
      _importing = true;
      _importStatus = '正在检查 QQ/微信目录…';
    });
    try {
      final source = WindowsImportSource();
      final candidates = await source.discoverCandidates(
          rememberedDirectories: await _preferences.qqImportDirectories());
      final batches = <_ImportBatch>[];
      var discovered = 0;
      for (final directory in candidates) {
        if (mounted) {
          setState(() => _importStatus = '正在扫描 ${directory.path}…');
        }
        final files = await source.scan(directory,
            maxFiles: WindowsImportSource.defaultMaxFiles);
        if (files.isEmpty) continue;
        discovered += files.length;
        batches.add(_ImportBatch(
          label: directory.path,
          source: source.sourceFor(directory),
          files: files,
        ));
      }
      if (discovered == 0) {
        if (mounted) {
          _showMessage('未找到可导入的 QQ/微信表情目录，请改用“选择表情文件夹”。');
        }
        return;
      }
      final selections =
          await _confirmImportPreview(title: '确认导入 QQ/微信表情', batches: batches);
      if (selections == null || selections.isEmpty) {
        return;
      }
      var added = 0;
      var duplicates = 0;
      var skipped = 0;
      var skippedTooLarge = 0;
      var skippedByTotalLimit = 0;
      for (final selection in selections) {
        final batch = selection.batch;
        if (mounted) {
          setState(() => _importStatus = '正在导入 ${selection.files.length} 个文件…');
        }
        final outcome = await _mediaStore.importFiles(
          selection.files,
          source: batch.source,
          groupIds: groupIds,
          onProgress: _reportImportProgress,
          onRecordsCommitted: _refreshAfterCommit,
        );
        added += outcome.added;
        duplicates += outcome.duplicates;
        skipped += outcome.skipped;
        skippedTooLarge += outcome.skippedTooLarge;
        skippedByTotalLimit += outcome.skippedByTotalLimit;
      }
      await _load();
      if (mounted) {
        _showMessage(
            '已扫描 ${batches.length} 个明确目录：导入 $added 个，重复 $duplicates 个，跳过 $skipped 个${_sizeLimitSuffix(skippedTooLarge, skippedByTotalLimit)}（每个目录最多扫描 ${WindowsImportSource.defaultMaxFiles} 个）');
      }
    } on Object catch (error) {
      if (mounted) _showMessage('导入失败：$error');
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

  Future<void> _importDirectory(Iterable<String> groupIds) async {
    String? directoryPath;
    try {
      directoryPath = await FilePicker.getDirectoryPath(
        dialogTitle: '选择 QQ/微信个人表情文件夹',
      );
    } on Object catch (error) {
      if (mounted) _showMessage('打开文件夹选择器失败：$error');
      return;
    }
    if (!mounted || directoryPath == null || directoryPath.isEmpty) return;

    final directory = Directory(directoryPath);
    final source = WindowsImportSource();
    setState(() {
      _importing = true;
      _importStatus = '正在扫描 ${directory.path}…';
    });
    try {
      final files = await source.scan(directory,
          maxFiles: WindowsImportSource.defaultMaxFiles);
      if (files.isEmpty) {
        if (mounted) _showMessage('所选文件夹中没有可识别的图片或 GIF。');
        return;
      }
      final batch = _ImportBatch(
        label: directory.path,
        source: source.sourceFor(directory),
        files: files,
      );
      final selections =
          await _confirmImportPreview(title: '确认导入文件夹', batches: [batch]);
      if (selections == null || selections.isEmpty) {
        return;
      }
      final selectedFiles = selections.single.files;
      if (mounted) {
        setState(() => _importStatus = '正在导入 ${selectedFiles.length} 个文件…');
      }
      final outcome = await _mediaStore.importFiles(
        selectedFiles,
        source: source.sourceFor(directory),
        groupIds: groupIds,
        onProgress: _reportImportProgress,
        onRecordsCommitted: _refreshAfterCommit,
      );
      if (source.sourceFor(directory) == StickerSource.qq) {
        await _preferences.rememberQqImportDirectory(directory.path);
      }
      await _load();
      if (mounted) {
        _showMessage(
            '从文件夹导入 ${outcome.added} 个，重复 ${outcome.duplicates} 个，跳过 ${outcome.skipped} 个${_sizeLimitSuffix(outcome.skippedTooLarge, outcome.skippedByTotalLimit)}');
      }
    } on Object catch (error) {
      if (mounted) _showMessage('导入失败：$error');
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

  Future<void> _importSelectedFiles(Iterable<String> groupIds) async {
    if (_importing) return;
    setState(() {
      _importing = true;
      _importStatus = '正在打开文件选择器…';
    });
    try {
      final result = await FilePicker.pickFiles(
        dialogTitle: '选择图片或 GIF 文件',
        // file_picker 12.x defaults pickFiles to multi-select. The single-file
        // API is pickFile; using pickFiles keeps this workflow multi-select.
        type: FileType.custom,
        allowedExtensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp', 'gif'],
      );
      final files = result
          .where((file) => file.path != null)
          .map((file) => File(file.path!))
          .toList(growable: false);
      if (files.isEmpty) return;
      final batch = _ImportBatch(
        label: '已选择的文件',
        source: StickerSource.manual,
        files: files,
      );
      final selections =
          await _confirmImportPreview(title: '确认导入表情文件', batches: [batch]);
      if (selections == null || selections.isEmpty) {
        return;
      }
      final selectedFiles = selections.single.files;
      if (mounted) {
        setState(() => _importStatus = '正在导入 ${selectedFiles.length} 个文件…');
      }
      final outcome = await _mediaStore.importFiles(
        selectedFiles,
        groupIds: groupIds,
        onProgress: _reportImportProgress,
        onRecordsCommitted: _refreshAfterCommit,
      );
      if (mounted) {
        _showMessage(
            '导入 ${outcome.added} 个，重复 ${outcome.duplicates} 个，跳过 ${outcome.skipped} 个${_sizeLimitSuffix(outcome.skippedTooLarge, outcome.skippedByTotalLimit)}');
      }
    } on Object catch (error) {
      if (mounted) _showMessage('导入失败：$error');
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

  Future<List<_ImportSelection>?> _confirmImportPreview({
    required String title,
    required List<_ImportBatch> batches,
  }) async {
    if (!mounted || batches.isEmpty) return null;
    final selectedFiles = <int, Set<int>>{
      for (var batchIndex = 0; batchIndex < batches.length; batchIndex++)
        batchIndex: <int>{
          for (var fileIndex = 0;
              fileIndex < batches[batchIndex].files.length;
              fileIndex++)
            if (batches[batchIndex].defaultSelected &&
                defaultSelectImportFile(batches[batchIndex].files[fileIndex]))
              fileIndex,
        },
    };
    final confirmed = await showDialog<List<_ImportSelection>>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (dialogContext, setDialogState) => AlertDialog(
          title: Text(title),
          content: SizedBox(
            width: 520,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('选择要导入的目录和文件。导入时会按内容哈希自动跳过重复项，不会修改源文件。'),
                const SizedBox(height: 12),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxHeight: 280),
                  child: ListView.separated(
                    shrinkWrap: true,
                    itemCount: batches.length,
                    separatorBuilder: (_, __) => const Divider(height: 1),
                    itemBuilder: (context, index) {
                      final batch = batches[index];
                      final selected = selectedFiles[index]!;
                      final allSelected = selected.length == batch.files.length;
                      return ListTile(
                        dense: true,
                        contentPadding: EdgeInsets.zero,
                        leading: Checkbox(
                          value: allSelected
                              ? true
                              : selected.isEmpty
                                  ? false
                                  : null,
                          tristate: true,
                          onChanged: (value) {
                            setDialogState(() {
                              selectedFiles[index] = value == true
                                  ? <int>{
                                      ...List<int>.generate(
                                          batch.files.length, (i) => i)
                                    }
                                  : <int>{};
                            });
                          },
                        ),
                        title: Text(_sourceLabel(batch.source)),
                        subtitle: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(batch.label,
                                maxLines: 2, overflow: TextOverflow.ellipsis),
                            if (batch.isReceivedOrMarketPath) ...[
                              const SizedBox(height: 2),
                              const Text('可能包含群聊或市场表情，默认不导入',
                                  style: TextStyle(fontSize: 12)),
                            ],
                            const SizedBox(height: 6),
                            SizedBox(
                              height: 64,
                              child: ListView.separated(
                                scrollDirection: Axis.horizontal,
                                itemCount: math.min(batch.files.length, 8),
                                separatorBuilder: (_, __) =>
                                    const SizedBox(width: 6),
                                itemBuilder: (context, fileIndex) {
                                  final file = batch.files[fileIndex];
                                  return ClipRRect(
                                    borderRadius: BorderRadius.circular(4),
                                    child: Image.file(
                                      file,
                                      width: 64,
                                      height: 64,
                                      fit: BoxFit.cover,
                                      cacheWidth: 128,
                                      cacheHeight: 128,
                                      filterQuality: FilterQuality.low,
                                      errorBuilder: (_, __, ___) => Container(
                                        width: 64,
                                        height: 64,
                                        color: Theme.of(context)
                                            .colorScheme
                                            .surfaceContainerHighest,
                                        alignment: Alignment.center,
                                        child: const Icon(
                                            Icons.broken_image_outlined,
                                            size: 20),
                                      ),
                                    ),
                                  );
                                },
                              ),
                            ),
                            const SizedBox(height: 4),
                            Text(
                                '${selected.length}/${batch.files.length} 个已选择'),
                          ],
                        ),
                        trailing: IconButton(
                          tooltip: '选择文件',
                          icon: const Icon(Icons.checklist_outlined),
                          onPressed: batch.files.isEmpty
                              ? null
                              : () async {
                                  final result = await _selectImportFiles(
                                    batch,
                                    selected,
                                  );
                                  if (result == null) return;
                                  setDialogState(
                                      () => selectedFiles[index] = result);
                                },
                        ),
                      );
                    },
                  ),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: selectedFiles.values.every((files) => files.isEmpty)
                  ? null
                  : () {
                      final selections = <_ImportSelection>[];
                      for (var index = 0; index < batches.length; index++) {
                        final indexes = selectedFiles[index]!;
                        if (indexes.isEmpty) continue;
                        selections.add(_ImportSelection(
                          batch: batches[index],
                          files:
                              selectImportFiles(batches[index].files, indexes),
                        ));
                      }
                      Navigator.pop(dialogContext, selections);
                    },
              child: const Text('开始导入'),
            ),
          ],
        ),
      ),
    );
    return confirmed;
  }

  Future<Set<int>?> _selectImportFiles(
      _ImportBatch batch, Set<int> initialSelection) {
    final selected = Set<int>.of(initialSelection);
    return showDialog<Set<int>>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setState) => AlertDialog(
          title: Text('选择 ${_sourceLabel(batch.source)} 文件'),
          content: SizedBox(
            width: 640,
            height: 520,
            child: GridView.builder(
              gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
                maxCrossAxisExtent: 120,
                mainAxisExtent: 132,
                crossAxisSpacing: 10,
                mainAxisSpacing: 10,
              ),
              itemCount: batch.files.length,
              itemBuilder: (context, index) {
                final file = batch.files[index];
                final isSelected = selected.contains(index);
                return InkWell(
                  onTap: () => setState(() {
                    if (isSelected) {
                      selected.remove(index);
                    } else {
                      selected.add(index);
                    }
                  }),
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      border: Border.all(
                        color: isSelected
                            ? Theme.of(context).colorScheme.primary
                            : Theme.of(context).colorScheme.outlineVariant,
                        width: isSelected ? 2 : 1,
                      ),
                      borderRadius: BorderRadius.circular(6),
                    ),
                    child: Stack(
                      children: [
                        Positioned.fill(
                          child: Padding(
                            padding: const EdgeInsets.all(4),
                            child: Image.file(
                              file,
                              fit: BoxFit.contain,
                              cacheWidth: 160,
                              cacheHeight: 160,
                              errorBuilder: (_, __, ___) => const Center(
                                  child: Icon(Icons.broken_image_outlined)),
                            ),
                          ),
                        ),
                        Positioned(
                          right: 0,
                          top: 0,
                          child: Checkbox(
                            value: isSelected,
                            onChanged: (value) => setState(() {
                              if (value == true) {
                                selected.add(index);
                              } else {
                                selected.remove(index);
                              }
                            }),
                          ),
                        ),
                        Positioned(
                          left: 4,
                          right: 4,
                          bottom: 2,
                          child: Text(
                            path.basename(file.path),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: Theme.of(context).textTheme.labelSmall,
                          ),
                        ),
                      ],
                    ),
                  ),
                );
              },
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(dialogContext, Set.of(selected)),
              child: Text('确定（${selected.length}）'),
            ),
          ],
        ),
      ),
    );
  }

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
      final outcome = await _mediaStore.importFiles(
        sharedPaths.map(File.new),
        source: StickerSource.androidShare,
        groupIds: groupIds,
        onProgress: _reportImportProgress,
        onRecordsCommitted: _refreshAfterCommit,
      );
      await PlatformBridge.instance.acknowledgeSharedFiles(sharedPaths);
      importCompleted = true;
      if (mounted) {
        final suffix =
            shareErrors.isEmpty ? '' : '；${_shareErrorMessage(shareErrors)}';
        _showMessage('已从分享导入 ${outcome.added} 个表情$suffix');
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

  String _shareErrorMessage(List<String> errors) {
    if (errors.isEmpty) return '';
    final first = errors.first.trim();
    final detail = first.isEmpty ? '' : '：$first';
    return '有 ${errors.length} 个分享文件未接收$detail';
  }

  String _sizeLimitSuffix(int tooLarge, int totalLimit) {
    final parts = <String>[];
    if (tooLarge > 0) parts.add('单文件过大 $tooLarge 个');
    if (totalLimit > 0) parts.add('批次大小超限 $totalLimit 个');
    return parts.isEmpty ? '' : '（${parts.join('，')}）';
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
    setState(() => _selectedGroup = groupId);
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
      setState(() => _selectedGroup = groupId);
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
              onPressed: () => Navigator.pop(context), child: const Text('取消')),
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
    final result = await PlatformBridge.instance.useSticker(entry.sticker);
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
  }

  /// Card copy button: clipboard only, no window activation, paste or Enter.
  /// Usage counting follows the platform rule: a Windows copy never counts,
  /// while on macOS and Android copying is itself a successful use.
  Future<void> _copySticker(RankedSticker entry) async {
    final result = await PlatformBridge.instance.copySticker(entry.sticker);
    if (result.succeeded && (Platform.isMacOS || Platform.isAndroid)) {
      await _repository.recordUsage(entry.sticker.id, DateTime.now());
      await _load();
    }
    if (!mounted) return;
    _showMessage(result.succeeded ? '已复制' : (result.message ?? '复制失败'));
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
    final action = await showDialog<String>(
      context: context,
      builder: (dialogContext) => SimpleDialog(
        title: const Text('设置'),
        children: [
          if (isDesktopPlatform)
            SimpleDialogOption(
              onPressed: () => Navigator.pop(dialogContext, 'hotkey'),
              child: const _SettingsEntry(Icons.keyboard_outlined, '设置快速唤出热键'),
            ),
          if (Platform.isWindows)
            SimpleDialogOption(
              onPressed: () => Navigator.pop(dialogContext, 'compatibility'),
              child:
                  const _SettingsEntry(Icons.fact_check_outlined, '剪贴板兼容性记录'),
            ),
          if (Platform.isAndroid)
            SimpleDialogOption(
              onPressed: () => Navigator.pop(dialogContext, 'floating_panel'),
              child: _SettingsEntry(
                Icons.picture_in_picture_alt_outlined,
                _floatingPanelEnabled ? '关闭悬浮面板' : '开启悬浮面板',
              ),
            ),
        ],
      ),
    );
    if (!mounted || action == null) return;
    switch (action) {
      case 'hotkey':
        await _configureHotKey();
      case 'compatibility':
        await _showCompatibilityRecords();
      case 'floating_panel':
        await _toggleFloatingPanel();
    }
  }

  @override
  Widget build(BuildContext context) {
    final visible = _visibleStickers;
    return _quickPickerMode
        ? _buildQuickPickerScaffold(visible)
        : _buildManagementScaffold(visible);
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
                              searchController: _searchController,
                              onSearchChanged: (_) =>
                                  setState(_resetKeyboardFocus),
                              onClearSearch: _clearSearch,
                              onEnterSelection: _enterSelectionMode,
                              onImport: _importFiles,
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
                              child: _buildContentArea(visible, searchQuery)),
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

  Widget _buildContentArea(List<RankedSticker> visible, String searchQuery) {
    if (_loading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_error != null) return _buildLoadError();
    return Column(
      children: [
        _buildContentHeader(visible, searchQuery),
        Expanded(
          child: visible.isEmpty
              ? (searchQuery.isNotEmpty
                  ? _SearchEmptyState(onClearSearch: _clearSearch)
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
    final countLabel = searchQuery.isEmpty
        ? '${visible.length} 张表情'
        : '搜索到 ${visible.length} 个表情';
    return Padding(
      padding: const EdgeInsets.fromLTRB(
          AppTheme.contentPadding, 24, AppTheme.contentPadding, 0),
      child: LayoutBuilder(builder: (context, constraints) {
        final narrow = constraints.maxWidth < 550;
        final title = Row(
          crossAxisAlignment: CrossAxisAlignment.baseline,
          textBaseline: TextBaseline.alphabetic,
          children: [
            Flexible(
              child: Text(
                _currentGroupName,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style:
                    const TextStyle(fontSize: 28, fontWeight: FontWeight.w600),
              ),
            ),
            const SizedBox(width: 24),
            Text(
              countLabel,
              style:
                  const TextStyle(fontSize: 16, color: AppTheme.secondaryText),
            ),
          ],
        );
        final controls = Row(mainAxisSize: MainAxisSize.min, children: [
          _buildSortControl(),
          const SizedBox(width: 16),
          _buildDensityControl(),
        ]);
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
        return Row(children: [
          Expanded(child: title),
          const SizedBox(width: 24),
          controls,
        ]);
      }),
    );
  }

  Widget _buildSortControl() {
    final defaultLabel = _selectedGroup == 'qq_favorites' ? '来源顺序' : '常用优先';
    return Container(
      width: 164,
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
                fontSize: 18,
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
                            ? (searchQuery.isNotEmpty
                                ? _SearchEmptyState(onClearSearch: _clearSearch)
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

  String _formatCompatibilityTime(DateTime value) {
    final local = value.toLocal();
    String twoDigits(int number) => number.toString().padLeft(2, '0');
    return '${local.month}/${local.day} ${twoDigits(local.hour)}:${twoDigits(local.minute)}';
  }

  Future<void> _configureHotKey() async {
    if (!isDesktopPlatform) return;
    HotKey? recorded;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('设置快速唤出热键'),
        content: SizedBox(
          width: 320,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('请按下至少包含一个修饰键的组合键。'),
              const SizedBox(height: 16),
              HotKeyRecorder(
                initalHotKey: QuickPickerController.instance.hotKey,
                onHotKeyRecorded: (value) => recorded = value,
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('保存'),
          ),
        ],
      ),
    );
    if (confirmed != true || recorded == null || !mounted) return;
    final updated =
        await QuickPickerController.instance.updateHotKey(recorded!);
    if (mounted) {
      _showMessage(updated ? '快速唤出热键已更新' : '热键注册失败，可能与其他程序冲突');
    }
  }
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

class _ImportSelection {
  const _ImportSelection({required this.batch, required this.files});

  final _ImportBatch batch;
  final List<File> files;
}

class _SettingsEntry extends StatelessWidget {
  const _SettingsEntry(this.icon, this.label);

  final IconData icon;
  final String label;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Icon(icon, size: 20, color: AppTheme.secondaryText),
        const SizedBox(width: 12),
        Expanded(child: Text(label)),
      ],
    );
  }
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
  const _SearchEmptyState({required this.onClearSearch});

  final VoidCallback onClearSearch;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Icon(Icons.search_off, size: 56, color: AppTheme.secondaryText),
          const SizedBox(height: 12),
          const Text('没有匹配的表情',
              style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600)),
          const SizedBox(height: 6),
          const Text('可以尝试搜索备注或表情 ID'),
          const SizedBox(height: 16),
          TextButton(onPressed: onClearSearch, child: const Text('清除搜索')),
        ],
      ),
    );
  }
}
