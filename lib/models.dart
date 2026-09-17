enum StickerMediaType { image, gif }

/// Thumbnail grid density, persisted as a user preference.
enum GridDensity { standard, compact }

enum StickerSource { manual, qq, wechat, androidShare }

class Sticker {
  const Sticker({
    required this.id,
    required this.hash,
    required this.mediaType,
    required this.filePath,
    required this.thumbnailPath,
    required this.source,
    required this.createdAt,
    required this.updatedAt,
    this.note = '',
    this.usageCount = 0,
    this.lastUsedAt,
    this.isPinned = false,
    this.thumbnailVersion = 0,
    this.sourceOrder,
  });

  final String id;
  final String hash;
  final StickerMediaType mediaType;
  final String filePath;
  final String thumbnailPath;
  final StickerSource source;
  final String note;
  final DateTime createdAt;
  final DateTime updatedAt;
  final DateTime? lastUsedAt;
  final int usageCount;
  final bool isPinned;
  final int thumbnailVersion;

  /// Order observed in the source collection, used by the QQ 收藏 group.
  final int? sourceOrder;

  Sticker copyWith({
    DateTime? createdAt,
    String? note,
    DateTime? updatedAt,
    DateTime? lastUsedAt,
    int? usageCount,
    bool? isPinned,
    int? thumbnailVersion,
    int? sourceOrder,
  }) {
    return Sticker(
      id: id,
      hash: hash,
      mediaType: mediaType,
      filePath: filePath,
      thumbnailPath: thumbnailPath,
      source: source,
      note: note ?? this.note,
      createdAt: createdAt ?? this.createdAt,
      updatedAt: updatedAt ?? this.updatedAt,
      lastUsedAt: lastUsedAt ?? this.lastUsedAt,
      usageCount: usageCount ?? this.usageCount,
      isPinned: isPinned ?? this.isPinned,
      thumbnailVersion: thumbnailVersion ?? this.thumbnailVersion,
      sourceOrder: sourceOrder ?? this.sourceOrder,
    );
  }
}

class StickerGroup {
  const StickerGroup({
    required this.id,
    required this.name,
    required this.createdAt,
  });

  final String id;
  final String name;
  final DateTime createdAt;
}

class StickerGroupMembership {
  const StickerGroupMembership(
      {required this.stickerId, required this.groupId});

  final String stickerId;
  final String groupId;
}

class RankedSticker {
  const RankedSticker(this.sticker, this.groupIds);

  final Sticker sticker;
  final Set<String> groupIds;
}

class ClipboardCompatibilityRecord {
  const ClipboardCompatibilityRecord({
    required this.targetApplication,
    required this.mediaType,
    required this.status,
    required this.createdAt,
    this.message,
  });

  final String targetApplication;
  final StickerMediaType mediaType;
  final String status;
  final DateTime createdAt;
  final String? message;

  Map<String, Object?> toJson() => {
        'targetApplication': targetApplication,
        'mediaType': enumValue(mediaType),
        'status': status,
        'createdAt': createdAt.toIso8601String(),
        'message': message,
      };

  static ClipboardCompatibilityRecord? fromJson(Object? value) {
    if (value is! Map) return null;
    final target = value['targetApplication'];
    final status = value['status'];
    final createdAt = value['createdAt'];
    if (target is! String ||
        target.trim().isEmpty ||
        status is! String ||
        createdAt is! String) {
      return null;
    }
    final parsedAt = DateTime.tryParse(createdAt);
    if (parsedAt == null) return null;
    return ClipboardCompatibilityRecord(
      targetApplication: target,
      mediaType: mediaTypeFrom(value['mediaType'] is String
          ? value['mediaType'] as String
          : 'image'),
      status: status,
      createdAt: parsedAt,
      message: value['message'] is String ? value['message'] as String : null,
    );
  }
}

String enumValue(Object value) => value.toString().split('.').last;

StickerMediaType mediaTypeFrom(String value) =>
    value == 'gif' ? StickerMediaType.gif : StickerMediaType.image;

StickerSource sourceFrom(String value) {
  return StickerSource.values.firstWhere(
    (source) => enumValue(source) == value,
    orElse: () => StickerSource.manual,
  );
}
