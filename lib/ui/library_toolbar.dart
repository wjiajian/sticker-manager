import 'package:flutter/material.dart';

import 'app_theme.dart';

/// Top action bar of the management window: search, multi-select entry,
/// the overflow menu and the primary import action.
class LibraryToolbar extends StatelessWidget {
  const LibraryToolbar({
    super.key,
    required this.searchController,
    required this.onSearchChanged,
    required this.onClearSearch,
    required this.onEnterSelection,
    required this.onImport,
    required this.moreMenu,
    this.leading,
    this.fixedControlHeight = true,
  });

  final TextEditingController searchController;
  final ValueChanged<String> onSearchChanged;
  final VoidCallback onClearSearch;
  final VoidCallback onEnterSelection;
  final VoidCallback onImport;

  /// Overflow menu; supplied by the page so platform-specific items stay
  /// with the business logic.
  final Widget moreMenu;

  /// Optional leading control, e.g. a drawer button on narrow windows.
  final Widget? leading;

  /// Desktop pins controls to [AppTheme.controlHeight]; touch layouts let
  /// the search field grow with text scale instead.
  final bool fixedControlHeight;

  @override
  Widget build(BuildContext context) {
    final search = SizedBox(
      height: fixedControlHeight ? AppTheme.controlHeight : 48,
      child: TextField(
        controller: searchController,
        onChanged: onSearchChanged,
        decoration: InputDecoration(
          hintText: '搜索备注或表情 ID',
          prefixIcon:
              const Icon(Icons.search, size: 20, color: AppTheme.secondaryText),
          prefixIconConstraints:
              const BoxConstraints(minWidth: 40, minHeight: 38),
          suffixIcon: searchController.text.isEmpty
              ? null
              : IconButton(
                  onPressed: onClearSearch,
                  tooltip: '清除搜索',
                  icon: const Icon(Icons.clear, size: 18),
                ),
        ),
      ),
    );
    final actions = [
      OutlinedButton.icon(
        onPressed: onEnterSelection,
        icon: const Icon(Icons.checklist_outlined, size: 18),
        label: const Text('多选'),
      ),
      moreMenu,
      FilledButton.icon(
        onPressed: onImport,
        icon: const Icon(Icons.add, size: 18),
        label: const Text('导入表情'),
      ),
    ];
    return LayoutBuilder(builder: (context, constraints) {
      final narrow = constraints.maxWidth < 600;
      return Container(
        decoration: const BoxDecoration(
          color: AppTheme.cardBackground,
          border: Border(bottom: BorderSide(color: AppTheme.border)),
        ),
        padding: EdgeInsets.symmetric(
            horizontal: narrow ? 16 : AppTheme.contentPadding, vertical: 12),
        child: narrow
            ? Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(children: [
                    if (leading != null) ...[
                      leading!,
                      const SizedBox(width: 8)
                    ],
                    Expanded(child: search),
                  ]),
                  const SizedBox(height: 8),
                  Wrap(
                    alignment: WrapAlignment.end,
                    spacing: 8,
                    runSpacing: 8,
                    crossAxisAlignment: WrapCrossAlignment.center,
                    children: actions,
                  ),
                ],
              )
            : Row(children: [
                if (leading != null) ...[leading!, const SizedBox(width: 12)],
                Expanded(child: search),
                const SizedBox(width: 12),
                actions[0],
                const SizedBox(width: 8),
                actions[1],
                const SizedBox(width: 8),
                actions[2],
              ]),
      );
    });
  }
}

/// Replaces [LibraryToolbar] while multi-select is active.
class SelectionToolbar extends StatelessWidget {
  const SelectionToolbar({
    super.key,
    required this.selectedCount,
    required this.onSelectAll,
    required this.onMove,
    required this.onDelete,
    required this.onExit,
  });

  final int selectedCount;
  final VoidCallback onSelectAll;
  final VoidCallback? onMove;
  final VoidCallback? onDelete;
  final VoidCallback onExit;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(builder: (context, constraints) {
      final narrow = constraints.maxWidth < 600;
      final actions = narrow
          ? <Widget>[
              IconButton(
                  onPressed: onSelectAll,
                  tooltip: '全选当前列表',
                  icon: const Icon(Icons.select_all)),
              IconButton(
                  onPressed: onMove,
                  tooltip: '移动到分组',
                  icon: const Icon(Icons.drive_file_move_outlined)),
              IconButton(
                  onPressed: onDelete,
                  tooltip: '删除',
                  color: Colors.red.shade700,
                  icon: const Icon(Icons.delete_outline)),
            ]
          : <Widget>[
              TextButton.icon(
                  onPressed: onSelectAll,
                  icon: const Icon(Icons.select_all, size: 18),
                  label: const Text('全选当前列表')),
              TextButton.icon(
                  onPressed: onMove,
                  icon: const Icon(Icons.drive_file_move_outlined, size: 18),
                  label: const Text('移动到分组')),
              TextButton.icon(
                  onPressed: onDelete,
                  icon: const Icon(Icons.delete_outline, size: 18),
                  label: const Text('删除'),
                  style: TextButton.styleFrom(
                      foregroundColor: Colors.red.shade700)),
            ];
      return Container(
        decoration: const BoxDecoration(
          color: AppTheme.cardBackground,
          border: Border(bottom: BorderSide(color: AppTheme.border)),
        ),
        padding: EdgeInsets.symmetric(
            horizontal: narrow ? 16 : AppTheme.contentPadding, vertical: 12),
        child: Row(children: [
          Expanded(
              child: Text('已选 $selectedCount 个表情',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                      fontSize: 16, fontWeight: FontWeight.w600))),
          ...actions,
          IconButton(
              onPressed: onExit,
              tooltip: '退出多选',
              icon: const Icon(Icons.close, size: 20)),
        ]),
      );
    });
  }
}
