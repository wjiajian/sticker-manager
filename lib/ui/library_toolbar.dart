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
    this.searchAll = false,
    this.onScopeChanged,
    this.onFilter,
    this.filterCount = 0,
    this.importMenu,
  });

  final TextEditingController searchController;
  final ValueChanged<String> onSearchChanged;
  final VoidCallback onClearSearch;
  final VoidCallback onEnterSelection;
  final VoidCallback onImport;
  final bool searchAll;
  final ValueChanged<bool>? onScopeChanged;
  final VoidCallback? onFilter;
  final int filterCount;
  final Widget? importMenu;

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
    return LayoutBuilder(
      builder: (context, constraints) {
        final narrow = constraints.maxWidth < (onFilter == null ? 600 : 900);
        const controlHeight = AppTheme.controlHeight;
        final actionStyle = ButtonStyle(
          minimumSize: const WidgetStatePropertyAll(Size(0, controlHeight)),
          padding: WidgetStatePropertyAll(
            EdgeInsets.symmetric(horizontal: narrow ? 14 : 18),
          ),
          textStyle: WidgetStatePropertyAll(
            Theme.of(context).textTheme.labelLarge!.copyWith(
                  fontSize: narrow ? 15 : 16,
                  fontWeight: FontWeight.w400,
                ),
          ),
        );
        final search = SizedBox(
          height: fixedControlHeight ? controlHeight : null,
          child: TextField(
            controller: searchController,
            onChanged: onSearchChanged,
            style: TextStyle(fontSize: narrow ? 15 : 16),
            textAlignVertical: TextAlignVertical.center,
            decoration: InputDecoration(
              hintText: '搜索备注或表情 ID',
              hintStyle: TextStyle(
                fontSize: narrow ? 15 : 16,
                color: AppTheme.secondaryText,
              ),
              fillColor: AppTheme.searchBackground,
              contentPadding: const EdgeInsets.symmetric(
                horizontal: 14,
                vertical: 12,
              ),
              prefixIcon: const Icon(
                Icons.search,
                size: 22,
                color: AppTheme.primaryText,
              ),
              prefixIconConstraints: const BoxConstraints(
                minWidth: 48,
                minHeight: controlHeight,
              ),
              suffixIcon: searchController.text.isEmpty
                  ? null
                  : IconButton(
                      onPressed: onClearSearch,
                      tooltip: '清除搜索',
                      icon: const Icon(Icons.clear, size: 20),
                    ),
            ),
          ),
        );
        final actions = [
          if (onFilter != null)
            OutlinedButton.icon(
              onPressed: onFilter,
              style: actionStyle,
              icon: const Icon(Icons.filter_list, size: 20),
              label: Text(filterCount == 0 ? '筛选' : '筛选 · $filterCount'),
            ),
          OutlinedButton.icon(
            onPressed: onEnterSelection,
            style: actionStyle,
            icon: const Icon(
              Icons.check_box_outline_blank_rounded,
              size: 20,
              color: AppTheme.secondaryText,
            ),
            label: const Text('多选'),
          ),
          moreMenu,
          Row(mainAxisSize: MainAxisSize.min, children: [
            FilledButton.icon(
                onPressed: onImport,
                style: actionStyle,
                icon: const Icon(Icons.add, size: 22),
                label: const Text('导入表情')),
            if (importMenu != null) importMenu!,
          ]),
        ];
        final searchRow = Row(
          children: [
            if (leading != null) ...[leading!, const SizedBox(width: 8)],
            if (onScopeChanged != null) ...[
              DropdownButtonHideUnderline(
                child: DropdownButton<bool>(
                  value: searchAll,
                  onChanged: (value) {
                    if (value != null) onScopeChanged!(value);
                  },
                  items: const [
                    DropdownMenuItem(value: false, child: Text('当前分组')),
                    DropdownMenuItem(value: true, child: Text('全部表情')),
                  ],
                ),
              ),
              const SizedBox(width: 8),
            ],
            Expanded(child: search),
          ],
        );
        return Container(
          decoration: const BoxDecoration(
            color: AppTheme.background,
            border: Border(bottom: BorderSide(color: AppTheme.border)),
          ),
          padding: EdgeInsets.symmetric(
            horizontal: narrow ? 16 : AppTheme.contentPadding,
            vertical: 12,
          ),
          child: narrow
              ? Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    searchRow,
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
              : Row(
                  children: [
                    Expanded(child: searchRow),
                    for (final action in actions) ...[
                      const SizedBox(width: 12),
                      action,
                    ],
                  ],
                ),
        );
      },
    );
  }
}

/// Replaces [LibraryToolbar] while multi-select is active.
class SelectionToolbar extends StatelessWidget {
  const SelectionToolbar({
    super.key,
    required this.selectedCount,
    required this.onSelectAll,
    required this.onMove,
    this.onAdd,
    required this.onDelete,
    required this.onExit,
  });

  final int selectedCount;
  final VoidCallback onSelectAll;
  final VoidCallback? onMove;
  final VoidCallback? onAdd;
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
                  onPressed: onAdd,
                  tooltip: '添加到分组',
                  icon: const Icon(Icons.create_new_folder_outlined)),
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
                  onPressed: onAdd,
                  icon: const Icon(Icons.create_new_folder_outlined, size: 18),
                  label: const Text('添加到分组')),
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
        constraints: const BoxConstraints(minHeight: 72),
        decoration: const BoxDecoration(
          color: AppTheme.background,
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
