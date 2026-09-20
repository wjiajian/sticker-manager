import 'dart:io';

import 'package:path/path.dart' as path;
import 'package:path_provider/path_provider.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';

import '../models.dart';
import 'repository.dart';

class StickerDatabase implements StickerRepository {
  // Bump this whenever thumbnail generation changes. Version 2 rebuilds
  // thumbnails produced by the earlier decoder, which could retain black
  // artifacts from damaged JPEGs.
  static const currentThumbnailVersion = 2;
  static const currentSchemaVersion = 3;
  StickerDatabase({this.databasePath});

  final String? databasePath;
  Database? _database;

  Future<Database> get database async {
    if (_database != null) return _database!;
    if (Platform.isWindows || Platform.isLinux || Platform.isMacOS) {
      sqfliteFfiInit();
      databaseFactory = databaseFactoryFfi;
    }
    final directory = databasePath == null
        ? await getApplicationSupportDirectory()
        : Directory(path.dirname(databasePath!));
    await directory.create(recursive: true);
    _database = await openDatabase(
      databasePath ?? path.join(directory.path, 'stickers.db'),
      version: currentSchemaVersion,
      onCreate: (db, version) async {
        await db.execute('''
          CREATE TABLE stickers (
            id TEXT PRIMARY KEY,
            hash TEXT NOT NULL UNIQUE,
            media_type TEXT NOT NULL,
            file_path TEXT NOT NULL,
            thumbnail_path TEXT NOT NULL,
            thumbnail_version INTEGER NOT NULL DEFAULT 2,
            source TEXT NOT NULL,
            note TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            last_used_at INTEGER,
            usage_count INTEGER NOT NULL DEFAULT 0,
            is_pinned INTEGER NOT NULL DEFAULT 0
            ,source_order INTEGER
          )
        ''');
        await db.execute('''
          CREATE TABLE groups (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            created_at INTEGER NOT NULL
          )
        ''');
        await db.execute('''
          CREATE TABLE sticker_groups (
            sticker_id TEXT NOT NULL,
            group_id TEXT NOT NULL,
            PRIMARY KEY (sticker_id, group_id),
            FOREIGN KEY (sticker_id) REFERENCES stickers(id) ON DELETE CASCADE,
            FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE
          )
        ''');
        await db.insert('groups', {
          'id': 'all',
          'name': '全部',
          'created_at': DateTime.now().millisecondsSinceEpoch,
        });
        await db.insert('groups', {
          'id': 'qq_favorites',
          'name': 'QQ收藏',
          'created_at': DateTime.now().millisecondsSinceEpoch + 1,
        });
      },
      onUpgrade: (db, oldVersion, newVersion) async {
        if (oldVersion < 2) {
          await db.execute(
              'ALTER TABLE stickers ADD COLUMN thumbnail_version INTEGER NOT NULL DEFAULT 0');
        }
        if (oldVersion < 3) {
          await db
              .execute('ALTER TABLE stickers ADD COLUMN source_order INTEGER');
          await db.insert(
              'groups',
              {
                'id': 'qq_favorites',
                'name': 'QQ收藏',
                'created_at': DateTime.now().millisecondsSinceEpoch + 1,
              },
              conflictAlgorithm: ConflictAlgorithm.ignore);
        }
      },
    );
    return _database!;
  }

  @override
  Future<List<RankedSticker>> loadRanked() async {
    final db = await database;
    final rows = await db.query('stickers');
    final memberships = await db.query('sticker_groups');
    final groups = <String, Set<String>>{};
    for (final membership in memberships) {
      final stickerId = membership['sticker_id']! as String;
      groups
          .putIfAbsent(stickerId, () => <String>{})
          .add(membership['group_id']! as String);
    }
    return rows.map((row) {
      final sticker = Sticker(
        id: row['id']! as String,
        hash: row['hash']! as String,
        mediaType: mediaTypeFrom(row['media_type']! as String),
        filePath: row['file_path']! as String,
        thumbnailPath: row['thumbnail_path']! as String,
        thumbnailVersion: (row['thumbnail_version'] as int?) ?? 0,
        sourceOrder: row['source_order'] as int?,
        source: sourceFrom(row['source']! as String),
        note: row['note']! as String,
        createdAt:
            DateTime.fromMillisecondsSinceEpoch(row['created_at']! as int),
        updatedAt:
            DateTime.fromMillisecondsSinceEpoch(row['updated_at']! as int),
        lastUsedAt: row['last_used_at'] == null
            ? null
            : DateTime.fromMillisecondsSinceEpoch(row['last_used_at']! as int),
        usageCount: row['usage_count']! as int,
        isPinned: (row['is_pinned']! as int) == 1,
      );
      return RankedSticker(sticker, groups[sticker.id] ?? <String>{'all'});
    }).toList();
  }

  @override
  Future<List<StickerGroup>> loadGroups() async {
    final db = await database;
    final rows = await db.query('groups',
        orderBy:
            "CASE id WHEN 'all' THEN 0 WHEN 'qq_favorites' THEN 1 ELSE 2 END, created_at ASC");
    return rows
        .map((row) => StickerGroup(
              id: row['id']! as String,
              name: row['name']! as String,
              createdAt: DateTime.fromMillisecondsSinceEpoch(
                  row['created_at']! as int),
            ))
        .toList();
  }

  @override
  Future<bool> insertSticker(Sticker sticker,
      {Iterable<String> groupIds = const ['all']}) async {
    return (await insertStickers([sticker], groupIds: groupIds)).isNotEmpty;
  }

  @override
  Future<List<Sticker>> insertStickers(
    Iterable<Sticker> stickers, {
    Iterable<String> groupIds = const ['all'],
  }) async {
    final items = stickers.toList(growable: false);
    if (items.isEmpty) return const [];
    final groups = groupIds.toSet();
    final db = await database;
    final inserted = <Sticker>[];
    await db.transaction((transaction) async {
      for (final sticker in items) {
        final row = await transaction.insert(
          'stickers',
          _stickerRow(sticker),
          conflictAlgorithm: ConflictAlgorithm.ignore,
        );
        if (row == 0) continue;
        for (final groupId in {
          ...groups,
          if (sticker.source == StickerSource.qq) 'qq_favorites'
        }) {
          await transaction.insert(
              'sticker_groups',
              {
                'sticker_id': sticker.id,
                'group_id': groupId,
              },
              conflictAlgorithm: ConflictAlgorithm.ignore);
        }
        inserted.add(sticker);
      }
      // A duplicate media hash can still be newly associated with the QQ
      // collection (or the group selected for this import). Preserve that
      // relationship instead of treating the duplicate as a no-op.
      for (final sticker in items) {
        final existing = await transaction.query('stickers',
            columns: ['id'],
            where: 'hash = ?',
            whereArgs: [sticker.hash],
            limit: 1);
        if (existing.isEmpty) continue;
        final existingId = existing.first['id'] as String;
        for (final groupId in {
          ...groups,
          if (sticker.source == StickerSource.qq) 'qq_favorites'
        }) {
          await transaction.insert(
              'sticker_groups',
              {
                'sticker_id': existingId,
                'group_id': groupId,
              },
              conflictAlgorithm: ConflictAlgorithm.ignore);
        }
        if (sticker.source == StickerSource.qq && sticker.sourceOrder != null) {
          await transaction.update(
              'stickers',
              {
                'source': enumValue(StickerSource.qq),
                'source_order': sticker.sourceOrder,
              },
              where: 'id = ?',
              whereArgs: [existingId]);
        }
      }
    });
    return inserted;
  }

  Map<String, Object?> _stickerRow(Sticker sticker) {
    return {
      'id': sticker.id,
      'hash': sticker.hash,
      'media_type': enumValue(sticker.mediaType),
      'file_path': sticker.filePath,
      'thumbnail_path': sticker.thumbnailPath,
      'thumbnail_version': sticker.thumbnailVersion,
      'source_order': sticker.sourceOrder,
      'source': enumValue(sticker.source),
      'note': sticker.note,
      'created_at': sticker.createdAt.millisecondsSinceEpoch,
      'updated_at': sticker.updatedAt.millisecondsSinceEpoch,
      'last_used_at': sticker.lastUsedAt?.millisecondsSinceEpoch,
      'usage_count': sticker.usageCount,
      'is_pinned': sticker.isPinned ? 1 : 0,
    };
  }

  @override
  Future<void> updateSticker(Sticker sticker) async {
    final db = await database;
    await db.update(
      'stickers',
      {
        'created_at': sticker.createdAt.millisecondsSinceEpoch,
        'note': sticker.note,
        'updated_at': sticker.updatedAt.millisecondsSinceEpoch,
        'last_used_at': sticker.lastUsedAt?.millisecondsSinceEpoch,
        'usage_count': sticker.usageCount,
        'is_pinned': sticker.isPinned ? 1 : 0,
        'thumbnail_path': sticker.thumbnailPath,
        'thumbnail_version': sticker.thumbnailVersion,
        'source_order': sticker.sourceOrder,
      },
      where: 'id = ?',
      whereArgs: [sticker.id],
    );
  }

  @override
  Future<void> updateThumbnail(
      String stickerId, String thumbnailPath, int thumbnailVersion) async {
    final db = await database;
    await db.update(
      'stickers',
      {
        'thumbnail_path': thumbnailPath,
        'thumbnail_version': thumbnailVersion,
        'updated_at': DateTime.now().millisecondsSinceEpoch,
      },
      where: 'id = ?',
      whereArgs: [stickerId],
    );
  }

  @override
  Future<void> recordUsage(String stickerId, DateTime usedAt) async {
    final db = await database;
    await db.rawUpdate(
      'UPDATE stickers '
      'SET usage_count = usage_count + 1, '
      'last_used_at = CASE '
      'WHEN last_used_at IS NULL OR last_used_at < ? THEN ? '
      'ELSE last_used_at END, updated_at = ? '
      'WHERE id = ?',
      [
        usedAt.millisecondsSinceEpoch,
        usedAt.millisecondsSinceEpoch,
        usedAt.millisecondsSinceEpoch,
        stickerId,
      ],
    );
  }

  @override
  Future<void> recordUsageMany(
      Iterable<String> stickerIds, DateTime usedAt) async {
    final counts = <String, int>{};
    for (final stickerId in stickerIds) {
      if (stickerId.isNotEmpty) {
        counts[stickerId] = (counts[stickerId] ?? 0) + 1;
      }
    }
    if (counts.isEmpty) return;
    final db = await database;
    final timestamp = usedAt.millisecondsSinceEpoch;
    await db.transaction((transaction) async {
      for (final entry in counts.entries) {
        await transaction.rawUpdate(
          'UPDATE stickers '
          'SET usage_count = usage_count + ?, '
          'last_used_at = CASE '
          'WHEN last_used_at IS NULL OR last_used_at < ? THEN ? '
          'ELSE last_used_at END, updated_at = ? '
          'WHERE id = ?',
          [entry.value, timestamp, timestamp, timestamp, entry.key],
        );
      }
    });
  }

  @override
  Future<void> deleteSticker(String stickerId) async {
    await deleteStickers([stickerId]);
  }

  @override
  Future<void> deleteStickers(Iterable<String> stickerIds) async {
    final ids = stickerIds.toList();
    if (ids.isEmpty) return;
    final db = await database;
    await db.transaction((transaction) async {
      // Do this explicitly instead of relying on SQLite foreign-key
      // enforcement, which is not enabled consistently by every driver.
      for (final id in ids) {
        await transaction
            .delete('sticker_groups', where: 'sticker_id = ?', whereArgs: [id]);
        await transaction.delete('stickers', where: 'id = ?', whereArgs: [id]);
      }
    });
  }

  @override
  Future<void> createGroup(String id, String name) async {
    final db = await database;
    await db.insert('groups', {
      'id': id,
      'name': name,
      'created_at': DateTime.now().millisecondsSinceEpoch,
    });
  }

  @override
  Future<void> attachGroup(String stickerId, String groupId) async {
    final db = await database;
    await db.insert(
        'sticker_groups', {'sticker_id': stickerId, 'group_id': groupId});
  }

  @override
  Future<void> replaceStickerGroups(
      String stickerId, Iterable<String> groupIds) async {
    await replaceStickerGroupsMany([stickerId], groupIds);
  }

  @override
  Future<void> attachGroupsMany(
      Iterable<String> stickerIds, Iterable<String> groupIds) async {
    final ids = stickerIds.toSet();
    final groups = groupIds.toSet();
    final db = await database;
    await db.transaction((transaction) async {
      for (final group in groups) {
        if ((await transaction.query('groups',
                columns: ['id'], where: 'id = ?', whereArgs: [group]))
            .isEmpty) {
          throw StateError('分组不存在：$group');
        }
      }
      for (final id in ids) {
        if ((await transaction.query('stickers',
                columns: ['id'], where: 'id = ?', whereArgs: [id]))
            .isEmpty) {
          throw StateError('表情不存在：$id');
        }
        for (final group in groups) {
          await transaction.insert(
              'sticker_groups', {'sticker_id': id, 'group_id': group},
              conflictAlgorithm: ConflictAlgorithm.ignore);
        }
      }
    });
  }

  @override
  Future<void> replaceStickerGroupsMany(
      Iterable<String> stickerIds, Iterable<String> groupIds) async {
    final ids = stickerIds.toSet();
    if (ids.isEmpty) return;
    final groups = <String>{'all', ...groupIds};
    final db = await database;
    await db.transaction((transaction) async {
      for (final stickerId in ids) {
        await transaction.delete('sticker_groups',
            where: 'sticker_id = ?', whereArgs: [stickerId]);
        for (final groupId in groups) {
          await transaction.insert('sticker_groups', {
            'sticker_id': stickerId,
            'group_id': groupId,
          });
        }
      }
    });
  }

  Future<void> close() async {
    final db = _database;
    _database = null;
    await db?.close();
  }
}
