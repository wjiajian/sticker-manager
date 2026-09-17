import 'dart:convert';

import 'package:hotkey_manager/hotkey_manager.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../models.dart';

class AppPreferences {
  static const _hotkeyKey = 'hotkey';
  static const _qqImportDirectoriesKey = 'qqImportDirectories';
  static const _compatibilityRecordsKey = 'clipboardCompatibilityRecords';
  static const _gridDensityKey = 'gridDensity';

  Future<String> hotkey() async {
    return (await SharedPreferences.getInstance()).getString(_hotkeyKey) ??
        'Ctrl+Shift+E';
  }

  Future<void> setHotkey(String value) async {
    await (await SharedPreferences.getInstance()).setString(_hotkeyKey, value);
  }

  Future<HotKey?> hotKeyConfig() async {
    final raw = (await SharedPreferences.getInstance()).getString(_hotkeyKey);
    if (raw == null || raw.trim().isEmpty) return null;
    try {
      final decoded = jsonDecode(raw);
      if (decoded is! Map) return null;
      return HotKey.fromJson(Map<String, dynamic>.from(decoded));
    } on Object {
      // Older versions stored a display string here. Ignore it and use the
      // default hotkey until the user records a new one.
      return null;
    }
  }

  Future<void> setHotKeyConfig(HotKey hotKey) async {
    await (await SharedPreferences.getInstance())
        .setString(_hotkeyKey, jsonEncode(hotKey.toJson()));
  }

  Future<List<String>> qqImportDirectories() async {
    final values = (await SharedPreferences.getInstance())
        .getStringList(_qqImportDirectoriesKey);
    return values ?? const <String>[];
  }

  Future<void> rememberQqImportDirectory(String directoryPath) async {
    final normalized = directoryPath.trim();
    if (normalized.isEmpty) return;
    final preferences = await SharedPreferences.getInstance();
    final existing =
        preferences.getStringList(_qqImportDirectoriesKey) ?? const <String>[];
    final values = <String>[normalized];
    for (final value in existing) {
      if (value.trim().isEmpty ||
          value.toLowerCase() == normalized.toLowerCase()) {
        continue;
      }
      values.add(value);
      if (values.length >= 12) break;
    }
    await preferences.setStringList(_qqImportDirectoriesKey, values);
  }

  Future<GridDensity> gridDensity() async {
    final raw =
        (await SharedPreferences.getInstance()).getString(_gridDensityKey);
    return raw == 'compact' ? GridDensity.compact : GridDensity.standard;
  }

  Future<void> setGridDensity(GridDensity density) async {
    await (await SharedPreferences.getInstance())
        .setString(_gridDensityKey, enumValue(density));
  }

  Future<List<ClipboardCompatibilityRecord>> compatibilityRecords() async {
    final preferences = await SharedPreferences.getInstance();
    final values =
        preferences.getStringList(_compatibilityRecordsKey) ?? const <String>[];
    final records = <ClipboardCompatibilityRecord>[];
    for (final value in values) {
      try {
        final record = ClipboardCompatibilityRecord.fromJson(jsonDecode(value));
        if (record != null) records.add(record);
      } on Object {
        // Ignore records written by an incomplete or older version.
      }
    }
    return records;
  }

  Future<void> recordCompatibility(ClipboardCompatibilityRecord record) async {
    final preferences = await SharedPreferences.getInstance();
    final existing =
        preferences.getStringList(_compatibilityRecordsKey) ?? const <String>[];
    final values = <String>[jsonEncode(record.toJson())];
    values.addAll(existing.take(99));
    await preferences.setStringList(_compatibilityRecordsKey, values);
  }
}
