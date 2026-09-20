import 'dart:convert';

import 'package:shared_preferences/shared_preferences.dart';

import 'async_mutex.dart';

/// Device-local history, deliberately independent of ranking and exports.
class RecentUsage {
  static const limit = 20;
  static const _key = 'recentStickerUses';
  final _lock = AsyncMutex();

  Future<List<String>> ids() async {
    final prefs = await SharedPreferences.getInstance();
    return _read(prefs).map((item) => item['id'] as String).toList();
  }

  List<Map<String, Object?>> _read(SharedPreferences prefs) {
    final result = <Map<String, Object?>>[];
    final seen = <String>{};
    for (final raw in prefs.getStringList(_key) ?? <String>[]) {
      try {
        final value = jsonDecode(raw);
        if (value is Map && value['id'] is String && seen.add(value['id'])) {
          result.add({'id': value['id'], 'at': value['at']});
        }
      } on FormatException {
        // Ignore an incomplete local record.
      }
    }
    return result.take(limit).toList();
  }

  Future<void> record(String id, {DateTime? at}) => _lock.protect(() async {
        final prefs = await SharedPreferences.getInstance();
        final values = [
          {'id': id, 'at': (at ?? DateTime.now()).toIso8601String()},
          ..._read(prefs).where((item) => item['id'] != id),
        ].take(limit);
        await prefs.setStringList(_key, values.map(jsonEncode).toList());
      });

  Future<void> retain(Set<String> ids) => _lock.protect(() async {
        final prefs = await SharedPreferences.getInstance();
        final values = _read(prefs);
        final retained =
            values.where((item) => ids.contains(item['id'])).toList();
        if (retained.length != values.length) {
          await prefs.setStringList(_key, retained.map(jsonEncode).toList());
        }
      });
}
