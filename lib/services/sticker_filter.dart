import '../models.dart';

enum ImportTimeRange { any, today, last7Days, last30Days, custom }

/// Session-only criteria. Calendar boundaries use the device's local time.
class StickerFilter {
  const StickerFilter({
    this.mediaType,
    this.sources = const {},
    this.timeRange = ImportTimeRange.any,
    this.startDate,
    this.endDate,
    this.pinnedOnly = false,
  });

  final StickerMediaType? mediaType;
  final Set<StickerSource> sources;
  final ImportTimeRange timeRange;
  final DateTime? startDate;
  final DateTime? endDate;
  final bool pinnedOnly;

  int get activeCount =>
      (mediaType == null ? 0 : 1) +
      (sources.isEmpty ? 0 : 1) +
      (timeRange == ImportTimeRange.any ? 0 : 1) +
      (pinnedOnly ? 1 : 0);

  bool matches(Sticker sticker, {required DateTime now}) {
    if (mediaType != null && mediaType != sticker.mediaType) return false;
    if (sources.isNotEmpty && !sources.contains(sticker.source)) return false;
    if (pinnedOnly && !sticker.isPinned) return false;
    if (timeRange == ImportTimeRange.any) return true;
    final localNow = now.toLocal();
    final today = DateTime(localNow.year, localNow.month, localNow.day);
    final DateTime start;
    final DateTime end;
    if (timeRange == ImportTimeRange.custom) {
      if (startDate == null || endDate == null) return false;
      start = DateTime(startDate!.year, startDate!.month, startDate!.day);
      end = DateTime(endDate!.year, endDate!.month, endDate!.day + 1);
    } else {
      final days = switch (timeRange) {
        ImportTimeRange.last7Days => 7,
        ImportTimeRange.last30Days => 30,
        _ => 1,
      };
      start = DateTime(today.year, today.month, today.day - days + 1);
      end = DateTime(today.year, today.month, today.day + 1);
    }
    final importedAt = sticker.createdAt.toLocal();
    return !importedAt.isBefore(start) && importedAt.isBefore(end);
  }

  String get summary => [
        if (mediaType != null)
          mediaType == StickerMediaType.gif ? 'GIF' : '静态图',
        if (sources.isNotEmpty)
          StickerSource.values
              .where(sources.contains)
              .map(sourceLabel)
              .join('、'),
        if (timeRange != ImportTimeRange.any)
          timeRange == ImportTimeRange.custom
              ? '${dateLabel(startDate!)}—${dateLabel(endDate!)}'
              : timeLabel(timeRange),
        if (pinnedOnly) '仅置顶',
      ].join(' · ');

  static String dateLabel(DateTime date) =>
      '${date.year}/${date.month}/${date.day}';

  static String timeLabel(ImportTimeRange range) => switch (range) {
        ImportTimeRange.any => '不限',
        ImportTimeRange.today => '今天',
        ImportTimeRange.last7Days => '最近 7 天',
        ImportTimeRange.last30Days => '最近 30 天',
        ImportTimeRange.custom => '自定义',
      };

  static String sourceLabel(StickerSource source) => switch (source) {
        StickerSource.manual => '手动导入',
        StickerSource.qq => 'QQ',
        StickerSource.wechat => '微信',
        StickerSource.androidShare => 'Android 分享',
      };
}
