import '../models.dart';

/// Sort rule applied after group and query filtering.
///
/// [defaultRule] keeps the per-group defaults (QQ 收藏按来源顺序，其余按置顶
/// 与使用频率)；[recentImport] orders by import time.
enum StickerSortOrder { defaultRule, recentImport }

class UsageRankingService {
  List<RankedSticker> rank(
    Iterable<RankedSticker> stickers, {
    String? groupId,
    String query = '',
    StickerSortOrder order = StickerSortOrder.defaultRule,
  }) {
    final normalizedQuery = query.trim().toLowerCase();
    final filtered = stickers.where((entry) {
      final inGroup = groupId == null || entry.groupIds.contains(groupId);
      final matchesQuery = normalizedQuery.isEmpty ||
          entry.sticker.note.toLowerCase().contains(normalizedQuery) ||
          entry.sticker.id.toLowerCase().contains(normalizedQuery);
      return inGroup && matchesQuery;
    }).toList();

    if (order == StickerSortOrder.recentImport) {
      filtered
          .sort((a, b) => b.sticker.createdAt.compareTo(a.sticker.createdAt));
      return filtered;
    }

    filtered.sort((a, b) {
      if (groupId == 'qq_favorites') {
        final orderA = a.sticker.sourceOrder;
        final orderB = b.sticker.sourceOrder;
        if (orderA != null || orderB != null) {
          if (orderA == null) return 1;
          if (orderB == null) return -1;
          final sourceOrder = orderA.compareTo(orderB);
          if (sourceOrder != 0) return sourceOrder;
        }
        return a.sticker.createdAt.compareTo(b.sticker.createdAt);
      }
      final pin =
          (b.sticker.isPinned ? 1 : 0).compareTo(a.sticker.isPinned ? 1 : 0);
      if (pin != 0) return pin;
      final count = b.sticker.usageCount.compareTo(a.sticker.usageCount);
      if (count != 0) return count;
      final recent = (b.sticker.lastUsedAt ??
              DateTime.fromMillisecondsSinceEpoch(0))
          .compareTo(
              a.sticker.lastUsedAt ?? DateTime.fromMillisecondsSinceEpoch(0));
      if (recent != 0) return recent;
      return b.sticker.createdAt.compareTo(a.sticker.createdAt);
    });
    return filtered;
  }
}
