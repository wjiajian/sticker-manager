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
            padding: EdgeInsets.fromLTRB(28, 24, 24, 32),
            child: Row(
              children: [
                _LibraryMark(),
                SizedBox(width: 16),
                Text(
                  '表情管家',
                  style: TextStyle(fontSize: 24, fontWeight: FontWeight.w600),
                ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 14),
            child: _SidebarItem(
              icon: Icons.grid_view_outlined,
              label: allGroupLabel,
              count: groupCounts[allGroupId] ?? 0,
              selected: selectedGroupId == allGroupId,
              onTap: () => onSelectGroup(allGroupId),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(28, 26, 22, 10),
            child: Row(
              children: [
                const Expanded(
                  child: Text(
                    '我的分组',
                    style:
                        TextStyle(fontSize: 18, color: AppTheme.secondaryText),
                  ),
                ),
                Container(
                  width: 32,
                  height: 32,
                  decoration: BoxDecoration(
                    color: AppTheme.cardBackground,
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: AppTheme.border),
                  ),
                  child: IconButton(
                    onPressed: onCreateGroup,
                    tooltip: '新建分组',
                    icon: const Icon(Icons.add, size: 22),
                    padding: EdgeInsets.zero,
                    visualDensity: VisualDensity.compact,
                    color: AppTheme.primaryText,
                  ),
                ),
              ],
            ),
          ),
          Expanded(
            child: ListView.builder(
              padding: const EdgeInsets.symmetric(horizontal: 14),
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
          Padding(
            padding: const EdgeInsets.fromLTRB(14, 12, 14, 28),
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
        color: selected ? AppTheme.selectionBackground : Colors.transparent,
        borderRadius: BorderRadius.circular(AppTheme.buttonRadius),
        child: InkWell(
          borderRadius: BorderRadius.circular(AppTheme.buttonRadius),
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 16),
            child: Row(
              children: [
                Icon(icon,
                    size: 26,
                    color: selected ? AppTheme.accent : AppTheme.primaryText),
                const SizedBox(width: 16),
                Expanded(
                  child: Text(
                    label,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 20,
                      fontWeight:
                          selected ? FontWeight.w500 : FontWeight.normal,
                      color: selected ? AppTheme.accent : AppTheme.primaryText,
                    ),
                  ),
                ),
                if (count != null)
                  Text(
                    '$count',
                    style: TextStyle(
                        fontSize: 17,
                        color: selected
                            ? AppTheme.accent
                            : AppTheme.secondaryText),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _LibraryMark extends StatelessWidget {
  const _LibraryMark();

  @override
  Widget build(BuildContext context) {
    return const SizedBox(
      width: 40,
      height: 44,
      child: CustomPaint(painter: _CatMarkPainter()),
    );
  }
}

class _CatMarkPainter extends CustomPainter {
  const _CatMarkPainter();

  @override
  void paint(Canvas canvas, Size size) {
    canvas.save();
    canvas.scale(size.width / 40, size.height / 44);
    final outline = Paint()
      ..color = AppTheme.accent
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.6
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    final head = Path()
      ..moveTo(6, 14)
      ..cubicTo(6, 8, 6, 3, 9, 3)
      ..lineTo(15, 8)
      ..quadraticBezierTo(20, 6, 25, 8)
      ..lineTo(31, 3)
      ..cubicTo(34, 3, 34, 8, 34, 14)
      ..cubicTo(42, 33, 33, 40, 20, 40)
      ..cubicTo(7, 40, -2, 33, 6, 14)
      ..close();
    canvas.drawPath(head, outline);
    final fill = Paint()..color = AppTheme.accent;
    canvas.drawCircle(const Offset(12.5, 22), 1.9, fill);
    canvas.drawCircle(const Offset(27.5, 22), 1.9, fill);
    canvas.drawOval(const Rect.fromLTWH(17.5, 25, 5, 3.5), fill);
    canvas.drawPath(
      Path()
        ..moveTo(20, 28)
        ..lineTo(20, 30)
        ..moveTo(15.5, 30)
        ..quadraticBezierTo(18, 33, 20, 30)
        ..quadraticBezierTo(22, 33, 24.5, 30),
      outline..strokeWidth = 2,
    );
    canvas.restore();
  }

  @override
  bool shouldRepaint(covariant _CatMarkPainter oldDelegate) => false;
}
