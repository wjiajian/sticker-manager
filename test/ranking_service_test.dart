import 'package:flutter_test/flutter_test.dart';

import 'package:sticker_manager/models.dart';
import 'package:sticker_manager/services/ranking_service.dart';

void main() {
  test('recent imports sort descending after group and query filtering', () {
    final old = Sticker(
      id: 'old',
      hash: 'old',
      mediaType: StickerMediaType.image,
      filePath: '',
      thumbnailPath: '',
      source: StickerSource.manual,
      note: '猫',
      createdAt: DateTime(2026),
      updatedAt: DateTime(2026),
      usageCount: 100,
      isPinned: true,
      sourceOrder: 0,
    );
    final recent = old.copyWith(createdAt: DateTime(2026, 9), sourceOrder: 1);
    final ranked = UsageRankingService().rank([
      RankedSticker(old, {'qq_favorites'}),
      RankedSticker(recent, {'qq_favorites'}),
      RankedSticker(recent.copyWith(note: '狗'), {'qq_favorites'}),
      RankedSticker(recent, {'other'}),
    ],
        groupId: 'qq_favorites',
        query: '猫',
        order: StickerSortOrder.recentImport);
    expect(ranked.map((item) => item.sticker.createdAt),
        [DateTime(2026, 9), DateTime(2026)]);
  });

  final now = DateTime(2026, 1, 1);

  Sticker sticker(String id,
      {int count = 0,
      bool pinned = false,
      String note = '',
      int? sourceOrder}) {
    return Sticker(
      id: id,
      hash: id,
      mediaType: StickerMediaType.image,
      filePath: '$id.png',
      thumbnailPath: '$id-thumb.png',
      source: StickerSource.manual,
      createdAt: now,
      updatedAt: now,
      usageCount: count,
      isPinned: pinned,
      note: note,
      sourceOrder: sourceOrder,
    );
  }

  test('pinned stickers sort before usage count and recency', () {
    final ranked = UsageRankingService().rank([
      RankedSticker(sticker('recent', count: 20), {'all'}),
      RankedSticker(sticker('pinned', count: 0, pinned: true), {'all'}),
      RankedSticker(sticker('common', count: 10), {'all'}),
    ]);
    expect(ranked.map((entry) => entry.sticker.id),
        ['pinned', 'recent', 'common']);
  });

  test('group and note filters are applied before ranking', () {
    final ranked = UsageRankingService().rank([
      RankedSticker(sticker('cat', note: '猫猫'), {'cats'}),
      RankedSticker(sticker('dog', note: '狗狗', count: 10), {'dogs'}),
    ], groupId: 'cats', query: '猫');
    expect(ranked.single.sticker.id, 'cat');
  });

  test('search matches trimmed case-insensitive notes such as 薇欧拉', () {
    final ranked = UsageRankingService().rank([
      RankedSticker(sticker('viola', note: '薇欧拉系列'), {'all', 'girls'}),
      RankedSticker(sticker('other', note: '薇欧拉系列'), {'all', 'boys'}),
      RankedSticker(sticker('id-match'), {'all', 'girls'}),
    ], groupId: 'girls', query: '  薇欧拉  ');

    expect(ranked.map((entry) => entry.sticker.id), ['viola']);
  });

  test('search includes the imported filename note and sticker id', () {
    final stickers = [
      RankedSticker(sticker('qq-image-001'), {'all'}),
      RankedSticker(sticker('manual', note: 'original-file-name'), {'all'}),
    ];

    expect(
        UsageRankingService()
            .rank(stickers, query: 'qq-image')
            .single
            .sticker
            .id,
        'qq-image-001');
    expect(
        UsageRankingService()
            .rank(stickers, query: 'ORIGINAL-FILE')
            .single
            .sticker
            .id,
        'manual');
  });

  test('QQ 收藏 keeps the source order ahead of usage ranking', () {
    final ranked = UsageRankingService().rank([
      RankedSticker(sticker('third', count: 100, sourceOrder: 2),
          {'all', 'qq_favorites'}),
      RankedSticker(sticker('first', count: 0, pinned: true, sourceOrder: 0),
          {'all', 'qq_favorites'}),
      RankedSticker(sticker('second', count: 50, sourceOrder: 1),
          {'all', 'qq_favorites'}),
    ], groupId: 'qq_favorites');
    expect(
        ranked.map((entry) => entry.sticker.id), ['first', 'second', 'third']);
  });
}
