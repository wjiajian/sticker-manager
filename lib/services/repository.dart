import '../models.dart';

/// Storage boundary used by the media and migration services.
///
/// Keeping persistence behind this interface leaves room for a future synced
/// repository without coupling the domain services to SQLite.
abstract interface class StickerRepository {
  Future<List<RankedSticker>> loadRanked();

  Future<List<StickerGroup>> loadGroups();

  Future<bool> insertSticker(Sticker sticker,
      {Iterable<String> groupIds = const ['all']});

  Future<List<Sticker>> insertStickers(Iterable<Sticker> stickers,
      {Iterable<String> groupIds = const ['all']});

  Future<void> updateSticker(Sticker sticker);

  Future<void> updateThumbnail(
      String stickerId, String thumbnailPath, int thumbnailVersion);

  Future<void> recordUsage(String stickerId, DateTime usedAt);

  Future<void> recordUsageMany(Iterable<String> stickerIds, DateTime usedAt);

  Future<void> deleteSticker(String stickerId);

  Future<void> deleteStickers(Iterable<String> stickerIds);

  Future<void> createGroup(String id, String name);

  Future<void> attachGroup(String stickerId, String groupId);

  Future<void> attachGroupsMany(
      Iterable<String> stickerIds, Iterable<String> groupIds);

  Future<void> replaceStickerGroups(
      String stickerId, Iterable<String> groupIds);

  Future<void> replaceStickerGroupsMany(
      Iterable<String> stickerIds, Iterable<String> groupIds);
}
