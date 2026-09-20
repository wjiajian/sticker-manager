import 'package:flutter_test/flutter_test.dart';
import 'package:sticker_manager/models.dart';
import 'package:sticker_manager/services/ranking_service.dart';
import 'package:sticker_manager/services/sticker_filter.dart';

void main() {
  final now = DateTime(2026, 9, 20, 12);
  Sticker sticker(
    String id, {
    DateTime? imported,
    StickerSource source = StickerSource.manual,
    StickerMediaType type = StickerMediaType.image,
    bool pinned = false,
    int count = 0,
    int? order,
  }) =>
      Sticker(
        id: id,
        hash: id,
        mediaType: type,
        filePath: '',
        thumbnailPath: '',
        source: source,
        createdAt: imported ?? now,
        updatedAt: now,
        isPinned: pinned,
        usageCount: count,
        sourceOrder: order,
        note: 'Cat',
      );

  test('criteria intersect while selected sources form a union', () {
    const filter = StickerFilter(
      mediaType: StickerMediaType.gif,
      sources: {StickerSource.qq, StickerSource.wechat},
      pinnedOnly: true,
      timeRange: ImportTimeRange.today,
    );
    final entries = [
      sticker(
        'qq',
        source: StickerSource.qq,
        type: StickerMediaType.gif,
        pinned: true,
      ),
      sticker(
        'wechat',
        source: StickerSource.wechat,
        type: StickerMediaType.gif,
        pinned: true,
      ),
      sticker('manual', type: StickerMediaType.gif, pinned: true),
      sticker('static', source: StickerSource.qq, pinned: true),
      sticker('unpinned', source: StickerSource.qq, type: StickerMediaType.gif),
      sticker(
        'yesterday',
        imported: DateTime(2026, 9, 19),
        source: StickerSource.qq,
        type: StickerMediaType.gif,
        pinned: true,
      ),
    ];
    expect(entries.where((s) => filter.matches(s, now: now)).map((s) => s.id), [
      'qq',
      'wechat',
    ]);
    expect(filter.activeCount, 4);
    expect(const StickerFilter().activeCount, 0);
    expect(
      entries.every((s) => const StickerFilter().matches(s, now: now)),
      isTrue,
    );
  });

  test('calendar ranges include first midnight and exclude next midnight', () {
    for (final entry in [
      (ImportTimeRange.today, DateTime(2026, 9, 20)),
      (ImportTimeRange.last7Days, DateTime(2026, 9, 14)),
      (ImportTimeRange.last30Days, DateTime(2026, 8, 22)),
    ]) {
      final filter = StickerFilter(timeRange: entry.$1);
      expect(
        filter.matches(sticker('start', imported: entry.$2), now: now),
        isTrue,
      );
      expect(
        filter.matches(
          sticker(
            'before',
            imported: entry.$2.subtract(const Duration(microseconds: 1)),
          ),
          now: now,
        ),
        isFalse,
      );
      expect(
        filter.matches(
          sticker('end', imported: DateTime(2026, 9, 20, 23, 59, 59)),
          now: now,
        ),
        isTrue,
      );
      expect(
        filter.matches(
          sticker('after', imported: DateTime(2026, 9, 21)),
          now: now,
        ),
        isFalse,
      );
    }
  });

  test('custom range includes whole final date and handles UTC timestamps', () {
    final filter = StickerFilter(
      timeRange: ImportTimeRange.custom,
      startDate: DateTime(2026, 8, 31),
      endDate: DateTime(2026, 9, 1),
    );
    expect(
      filter.matches(
        sticker('start', imported: DateTime(2026, 8, 31).toUtc()),
        now: now,
      ),
      isTrue,
    );
    expect(
      filter.matches(
        sticker('end', imported: DateTime(2026, 9, 1, 23, 59, 59).toUtc()),
        now: now,
      ),
      isTrue,
    );
    expect(
      filter.matches(
        sticker('after', imported: DateTime(2026, 9, 2)),
        now: now,
      ),
      isFalse,
    );
  });

  test('filter preserves group, query, and QQ source order', () {
    final entries = [
      RankedSticker(sticker('second', count: 99, order: 1), {'qq_favorites'}),
      RankedSticker(sticker('first', order: 0), {'qq_favorites'}),
      RankedSticker(sticker('other', order: 0), {'other'}),
      RankedSticker(sticker('gif', type: StickerMediaType.gif), {
        'qq_favorites',
      }),
    ];
    final result = UsageRankingService().rank(
      entries,
      groupId: 'qq_favorites',
      query: ' CAT ',
      filter: const StickerFilter(mediaType: StickerMediaType.image),
      now: now,
    );
    expect(result.map((s) => s.sticker.id), ['first', 'second']);
    final global = UsageRankingService().rank(
      entries,
      query: ' SECOND ',
      filter: const StickerFilter(mediaType: StickerMediaType.image),
      now: now,
    );
    expect(global.single.sticker.id, 'second');
  });
}
