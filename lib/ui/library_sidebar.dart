import 'package:flutter/material.dart';

import '../models.dart';
import 'app_theme.dart';

/// Left navigation of the management window: app identity, the 全部表情
/// entry, user groups with counts, group creation and the settings entry.
class LibrarySidebar extends StatelessWidget {
  const LibrarySidebar({
    super.key,
    required this.groups,
    required this.groupCounts,
    required this.selectedGroupId,
    required this.onSelectGroup,
    required this.onCreateGroup,
    required this.onOpenSettings,
  });

  final List<StickerGroup> groups;

  /// Sticker count per group id, including `all`. Counts derive from the
  /// loaded library; membership is many-to-many, so group counts can add up
  /// to more than the total.
  final Map<String, int> groupCounts;
  final String selectedGroupId;
  final ValueChanged<String> onSelectGroup;
  final VoidCallback onCreateGroup;
  final VoidCallback onOpenSettings;

  static const String allGroupId = 'all';
  static const String allGroupLabel = '全部表情';

  @override
  Widget build(BuildContext context) {
    final selectable = groups.where((group) => group.id != allGroupId).toList();
    return Container(
      width: AppTheme.sidebarWidth,
      color: AppTheme.sidebarBackground,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Padding(
            padding: EdgeInsets.fromLTRB(16, 16, 16, 20),
            child: Row(
              children: [
                Icon(Icons.collections_bookmark_outlined,
                    size: 26, color: AppTheme.accent),
                SizedBox(width: 10),
                Text(
                  '表情管家',
                  style: TextStyle(fontSize: 20, fontWeight: FontWeight.w600),
                ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: _SidebarItem(
              icon: Icons.grid_view_outlined,
              label: allGroupLabel,
              count: groupCounts[allGroupId] ?? 0,
              selected: selectedGroupId == allGroupId,
              onTap: () => onSelectGroup(allGroupId),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 20, 8, 4),
            child: Row(
              children: [
                const Expanded(
                  child: Text(
                    '我的分组',
                    style:
                        TextStyle(fontSize: 12, color: AppTheme.secondaryText),
                  ),
                ),
                SizedBox(
                  width: 28,
                  height: 28,
                  child: IconButton(
                    onPressed: onCreateGroup,
                    tooltip: '新建分组',
                    icon: const Icon(Icons.add, size: 18),
                    padding: EdgeInsets.zero,
                    visualDensity: VisualDensity.compact,
                    color: AppTheme.secondaryText,
                  ),
                ),
              ],
            ),
          ),
          Expanded(
            child: ListView.builder(
              padding: const EdgeInsets.symmetric(horizontal: 12),
              itemCount: selectable.length,
              itemBuilder: (context, index) {
                final group = selectable[index];
                return _SidebarItem(
                  icon: Icons.folder_outlined,
                  label: group.name,
                  count: groupCounts[group.id] ?? 0,
                  selected: selectedGroupId == group.id,
                  onTap: () => onSelectGroup(group.id),
                );
              },
            ),
          ),
          const Divider(height: 1, color: AppTheme.border),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            child: _SidebarItem(
              icon: Icons.settings_outlined,
              label: '设置',
              selected: false,
              onTap: onOpenSettings,
            ),
          ),
        ],
      ),
    );
  }
}

class _SidebarItem extends StatelessWidget {
  const _SidebarItem({
    required this.icon,
    required this.label,
    required this.selected,
    required this.onTap,
    this.count,
  });

  final IconData icon;
  final String label;
  final bool selected;
  final VoidCallback onTap;
  final int? count;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 2),
      child: Material(
        color: selected ? AppTheme.cardBackground : Colors.transparent,
        borderRadius: BorderRadius.circular(AppTheme.buttonRadius),
        child: InkWell(
          borderRadius: BorderRadius.circular(AppTheme.buttonRadius),
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 9),
            child: Row(
              children: [
                Icon(icon,
                    size: 20,
                    color: selected ? AppTheme.accent : AppTheme.secondaryText),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    label,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 14,
                      fontWeight:
                          selected ? FontWeight.w600 : FontWeight.normal,
                      color: selected ? AppTheme.accent : AppTheme.primaryText,
                    ),
                  ),
                ),
                if (count != null)
                  Text(
                    '$count',
                    style: const TextStyle(
                        fontSize: 12, color: AppTheme.secondaryText),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
