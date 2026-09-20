import 'dart:async';
import 'dart:io';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:crypto/crypto.dart';
import 'package:path/path.dart' as path;
import 'package:path_provider/path_provider.dart';

import '../models.dart';
import 'database.dart';
import 'import_source.dart';
import 'import_preview.dart';
import 'repository.dart';

class ImportProgress {
  const ImportProgress({
    required this.total,
    required this.prepared,
    required this.submitted,
    required this.thumbnailsGenerated,
    this.skippedTooLarge = 0,
    this.skippedByTotalLimit = 0,
  });

  final int total;
  final int prepared;
  final int submitted;
  final int thumbnailsGenerated;

  /// Files rejected before import because they exceeded a per-file limit.
  final int skippedTooLarge;

  /// Files rejected because accepting them would exceed the batch byte limit.
  final int skippedByTotalLimit;
}

class ImportResult {
  const ImportResult({
    required this.added,
    required this.duplicates,
    required this.skipped,
    required this.thumbnailsGenerated,
    this.existingGrouped = 0,
    this.skippedTooLarge = 0,
    this.skippedByTotalLimit = 0,
  });

  final int added;
  final int duplicates;
  final int existingGrouped;
  int get alreadyExists => duplicates - existingGrouped;
  final int skipped;
  final int thumbnailsGenerated;

  /// Number of files skipped by the per-file byte limit. This is additive to
  /// [skipped], preserving the original result contract for existing callers.
  final int skippedTooLarge;

  /// Number of files skipped by the aggregate import byte limit. This is
  /// additive to [skipped], preserving the original result contract.
  final int skippedByTotalLimit;

  int get sizeLimited => skippedTooLarge + skippedByTotalLimit;
}

class MediaStore {
  MediaStore(this.database, {Directory? mediaDirectory})
      : _providedMediaDirectory = mediaDirectory;

  final StickerRepository database;
  final Directory? _providedMediaDirectory;
  bool _legacyThumbnailJobRunning = false;

  static const _prepareConcurrency = 4;
  static const _thumbnailConcurrency = 2;

  /// Keep one malformed or unexpectedly huge source file from exhausting the
  /// process while still allowing ordinary QQ/WeChat GIFs and images.
  static const maxImportFileBytes = 64 * 1024 * 1024;

  /// A normal QQ collection (500, or 1,000 for SVIP) is generally well below
  /// this ceiling. The limit applies to the selected source bytes, before
  /// hash-based deduplication, so it also bounds transient work and disk use.
  static const maxImportTotalBytes = 512 * 1024 * 1024;

  static const _readChunkBytes = 64 * 1024;

  Future<Directory> get _mediaDirectory async {
    final providedDirectory = _providedMediaDirectory;
    if (providedDirectory != null) {
      await providedDirectory.create(recursive: true);
      return providedDirectory;
    }
    final root = await getApplicationSupportDirectory();
    final directory = Directory(path.join(root.path, 'media'));
    await directory.create(recursive: true);
    return directory;
  }

  /// Read-only validation. No managed files or records exist until confirmation.
  Future<List<ImportCandidate>> inspectFiles(
    Iterable<File> files, {
    StickerSource Function(File)? sourceForFile,
  }) async {
    final existing = {
      for (final e in await database.loadRanked()) e.sticker.hash: e
    };
    final result = <ImportCandidate>[];
    var total = 0;
    for (final file in files) {
      final source = sourceForFile?.call(file) ?? StickerSource.manual;
      final length = await _fileLength(file) ?? 0;
      String? error;
      String? hash;
      StickerMediaType? type;
      if (length <= 0) {
        error = '文件为空或无法读取';
      } else if (length > maxImportFileBytes) {
        error = '单文件超过 64 MiB';
      } else if (length > maxImportTotalBytes - total) {
        error = '批次超过 512 MiB';
      } else {
        total += length;
        final read = await _readBytesWithinLimit(file);
        if (read.bytes == null) {
          error = '文件无法读取或大小发生变化';
        } else {
          type = _mediaTypeFor(read.bytes!);
          if (type == null) {
            error = '不支持的图片格式';
          } else {
            hash = sha256.convert(read.bytes!).toString();
          }
        }
      }
      result.add(ImportCandidate(
          file: file,
          source: source,
          bytes: length,
          hash: hash,
          mediaType: type,
          existing: existing[hash],
          error: error));
    }
    return result;
  }

  Future<ImportResult> importFiles(
    Iterable<File> files, {
    StickerSource source = StickerSource.manual,
    StickerSource Function(File file)? sourceForFile,
    Map<String, String>? expectedHashes,
    Iterable<String> groupIds = const ['all'],
    void Function(ImportProgress progress)? onProgress,
    FutureOr<void> Function(List<Sticker> stickers)? onRecordsCommitted,
  }) async {
    final input = files.toList(growable: false);
    final importGroupIds = groupIds.toSet();
    final existing = {
      for (final e in await database.loadRanked()) e.sticker.hash: e
    };
    final target = await _mediaDirectory;
    final outcomes = List<_PreparationOutcome?>.filled(input.length, null);
    final candidates = <_PreparationInput>[];
    var reservedTotalBytes = 0;
    var skippedTooLarge = 0;
    var skippedByTotalLimit = 0;

    // Stat files before scheduling workers. This prevents a large source from
    // being read into memory just to discover that it cannot be imported, and
    // makes the aggregate limit deterministic for a multi-file selection.
    for (var index = 0; index < input.length; index++) {
      final file = input[index];
      final itemSource = sourceForFile?.call(file) ?? source;
      final length = await _fileLength(file);
      if (length == null || length <= 0) {
        outcomes[index] = const _PreparationOutcome.skipped();
        continue;
      }
      if (length > maxImportFileBytes) {
        outcomes[index] =
            const _PreparationOutcome.skipped(_PreparationSkipReason.tooLarge);
        continue;
      }
      if (length > maxImportTotalBytes - reservedTotalBytes) {
        outcomes[index] = const _PreparationOutcome.skipped(
            _PreparationSkipReason.totalLimit);
        continue;
      }
      reservedTotalBytes += length;
      candidates.add(_PreparationInput(index, file, itemSource));
    }

    var next = 0;

    Future<void> prepareWorker() async {
      while (true) {
        final candidateIndex = next++;
        if (candidateIndex >= candidates.length) return;
        final candidate = candidates[candidateIndex];
        outcomes[candidate.inputIndex] = await _prepare(
          candidate.file,
          candidate.source,
          target,
          existing: existing,
          expectedHash: expectedHashes?[candidate.file.path],
          sourceOrder: candidate.source == StickerSource.qq
              ? candidate.inputIndex
              : null,
        );
      }
    }

    final workerCount = candidates.length < _prepareConcurrency
        ? candidates.length
        : _prepareConcurrency;
    if (workerCount > 0) {
      await Future.wait(
          List<Future<void>>.generate(workerCount, (_) => prepareWorker()));
    }

    final prepared = <Sticker>[];
    var skipped = 0;
    for (final outcome in outcomes) {
      final sticker = outcome?.sticker;
      if (sticker != null) {
        prepared.add(sticker);
      } else {
        skipped++;
        if (outcome?.reason == _PreparationSkipReason.tooLarge) {
          skippedTooLarge++;
        } else if (outcome?.reason == _PreparationSkipReason.totalLimit) {
          skippedByTotalLimit++;
        }
      }
    }
    onProgress?.call(ImportProgress(
      total: input.length,
      prepared: prepared.length,
      submitted: 0,
      thumbnailsGenerated: 0,
      skippedTooLarge: skippedTooLarge,
      skippedByTotalLimit: skippedByTotalLimit,
    ));

    final associated = <String>{};
    for (final sticker in prepared) {
      final old = existing[sticker.hash];
      final desired = {
        ...importGroupIds,
        if (sticker.source == StickerSource.qq) 'qq_favorites'
      };
      if (old != null && !old.groupIds.containsAll(desired)) {
        associated.add(sticker.hash);
      }
    }
    final inserted = await database.insertStickers(
      prepared,
      groupIds: importGroupIds,
    );
    final duplicates = prepared.length - inserted.length;
    await onRecordsCommitted?.call(inserted);
    onProgress?.call(ImportProgress(
      total: input.length,
      prepared: prepared.length,
      submitted: inserted.length,
      thumbnailsGenerated: 0,
      skippedTooLarge: skippedTooLarge,
      skippedByTotalLimit: skippedByTotalLimit,
    ));

    final thumbnailsGenerated = await _generateThumbnails(
      inserted,
      target,
      onProgress: (generated) => onProgress?.call(ImportProgress(
        total: input.length,
        prepared: prepared.length,
        submitted: inserted.length,
        thumbnailsGenerated: generated,
        skippedTooLarge: skippedTooLarge,
        skippedByTotalLimit: skippedByTotalLimit,
      )),
    );
    return ImportResult(
      added: inserted.length,
      duplicates: duplicates,
      existingGrouped: associated.length,
      skipped: skipped,
      thumbnailsGenerated: thumbnailsGenerated,
      skippedTooLarge: skippedTooLarge,
      skippedByTotalLimit: skippedByTotalLimit,
    );
  }

  Future<int?> _fileLength(File file) async {
    try {
      final length = await file.length();
      return length >= 0 ? length : null;
    } on Object {
      return null;
    }
  }

  Future<_PreparationOutcome> _prepare(
      File file, StickerSource source, Directory target,
      {int? sourceOrder,
      Map<String, RankedSticker> existing = const {},
      String? expectedHash}) async {
    try {
      final read = await _readBytesWithinLimit(file);
      if (read.reason != null) {
        return _PreparationOutcome.skipped(read.reason!);
      }
      final bytes = read.bytes!;
      final mediaType = _mediaTypeFor(bytes);
      if (mediaType == null) return const _PreparationOutcome.skipped();
      final hash = sha256.convert(bytes).toString();
      if (expectedHash != null && hash != expectedHash) {
        return const _PreparationOutcome.skipped();
      }
      final extension = mediaType == StickerMediaType.gif ? 'gif' : 'image';
      final old = existing[hash]?.sticker;
      final destination =
          File(old?.filePath ?? path.join(target.path, '$hash.$extension'));
      if (old == null && !await destination.exists()) {
        await destination.writeAsBytes(bytes);
      } else if (old == null && await destination.length() != bytes.length) {
        // A previous interrupted import may have left a partial hash-named
        // file. Repair it before creating the database record.
        await destination.writeAsBytes(bytes);
      }
      final now = DateTime.now();
      return _PreparationOutcome(Sticker(
        id: hash.substring(0, 16),
        hash: hash,
        mediaType: mediaType,
        filePath: destination.path,
        thumbnailPath: '',
        thumbnailVersion: 0,
        sourceOrder: sourceOrder,
        source: source,
        createdAt: now,
        updatedAt: now,
        note: path.basenameWithoutExtension(file.path),
      ));
    } on Object {
      return const _PreparationOutcome.skipped();
    }
  }

  Future<_ReadFileOutcome> _readBytesWithinLimit(File file) async {
    RandomAccessFile? handle;
    try {
      handle = await file.open();
      final builder = BytesBuilder(copy: false);
      var total = 0;
      while (true) {
        final chunk = await handle.read(_readChunkBytes);
        if (chunk.isEmpty) break;
        total += chunk.length;
        if (total > maxImportFileBytes) {
          return const _ReadFileOutcome.skipped(
              _PreparationSkipReason.tooLarge);
        }
        builder.add(chunk);
      }
      final bytes = builder.takeBytes();
      if (bytes.isEmpty) return const _ReadFileOutcome.skipped();
      return _ReadFileOutcome.data(bytes);
    } on Object {
      return const _ReadFileOutcome.skipped();
    } finally {
      await handle?.close();
    }
  }

  Future<void> deleteSticker(Sticker sticker) async {
    await deleteStickers([sticker]);
  }

  Future<void> deleteStickers(Iterable<Sticker> stickers) async {
    final items = stickers.toList();
    if (items.isEmpty) return;
    final mediaRoot = await _mediaDirectory;
    await database.deleteStickers(items.map((sticker) => sticker.id));
    for (final sticker in items) {
      await _deleteManagedFile(sticker.filePath, mediaRoot);
      await _deleteManagedFile(sticker.thumbnailPath, mediaRoot);
    }
  }

  Future<int> rebuildLegacyThumbnails({
    void Function(int generated, int total)? onProgress,
  }) async {
    if (_legacyThumbnailJobRunning) return 0;
    _legacyThumbnailJobRunning = true;
    try {
      final entries = await database.loadRanked();
      final targets = entries
          .map((entry) => entry.sticker)
          .where((sticker) =>
              sticker.thumbnailVersion <
              StickerDatabase.currentThumbnailVersion)
          .toList(growable: false);
      if (targets.isEmpty) return 0;
      final generated = await _generateThumbnails(
        targets,
        await _mediaDirectory,
        onProgress: (value) => onProgress?.call(value, targets.length),
      );
      return generated;
    } finally {
      _legacyThumbnailJobRunning = false;
    }
  }

  Future<void> _deleteManagedFile(String filePath, Directory mediaRoot) async {
    if (filePath.isEmpty) return;
    final rootPath = path.normalize(mediaRoot.absolute.path).toLowerCase();
    final candidatePath =
        path.normalize(File(filePath).absolute.path).toLowerCase();
    final rootPrefix = '$rootPath${Platform.pathSeparator}';
    if (candidatePath != rootPath && !candidatePath.startsWith(rootPrefix)) {
      return;
    }
    try {
      await File(filePath).delete();
    } on FileSystemException {
      // A missing or locked generated thumbnail should not block record removal.
    }
  }

  StickerMediaType? _mediaTypeFor(Uint8List bytes) {
    if (bytes.length >= 6) {
      final signature = String.fromCharCodes(bytes.take(6));
      if (signature == 'GIF87a' || signature == 'GIF89a') {
        return StickerMediaType.gif;
      }
    }
    if (_hasImageSignature(bytes)) return StickerMediaType.image;
    return null;
  }

  bool _hasImageSignature(Uint8List bytes) {
    if (bytes.length >= 8 &&
        bytes[0] == 0x89 &&
        bytes[1] == 0x50 &&
        bytes[2] == 0x4e &&
        bytes[3] == 0x47 &&
        bytes[4] == 0x0d &&
        bytes[5] == 0x0a &&
        bytes[6] == 0x1a &&
        bytes[7] == 0x0a) {
      return true;
    }
    if (bytes.length >= 3 &&
        bytes[0] == 0xff &&
        bytes[1] == 0xd8 &&
        bytes[2] == 0xff) {
      return true;
    }
    if (bytes.length >= 12 &&
        String.fromCharCodes(bytes.sublist(0, 4)) == 'RIFF' &&
        String.fromCharCodes(bytes.sublist(8, 12)) == 'WEBP') {
      return true;
    }
    return bytes.length >= 2 && bytes[0] == 0x42 && bytes[1] == 0x4d;
  }

  Future<int> _generateThumbnails(
    List<Sticker> stickers,
    Directory target, {
    void Function(int generated)? onProgress,
  }) async {
    if (stickers.isEmpty) return 0;
    var next = 0;
    var generated = 0;
    var lastReported = DateTime.fromMillisecondsSinceEpoch(0);

    Future<void> worker() async {
      while (true) {
        final index = next++;
        if (index >= stickers.length) return;
        final sticker = stickers[index];
        var thumbnailPath = '';
        try {
          thumbnailPath = await _createThumbnail(sticker, target) ?? '';
          if (thumbnailPath.isNotEmpty) generated++;
        } on Object {
          thumbnailPath = '';
        }
        await database.updateThumbnail(
            sticker.id, thumbnailPath, StickerDatabase.currentThumbnailVersion);
        final now = DateTime.now();
        if (generated == stickers.length ||
            now.difference(lastReported) >= const Duration(milliseconds: 120)) {
          lastReported = now;
          onProgress?.call(generated);
        }
      }
    }

    final workerCount = stickers.length < _thumbnailConcurrency
        ? stickers.length
        : _thumbnailConcurrency;
    await Future.wait(
        List<Future<void>>.generate(workerCount, (_) => worker()));
    onProgress?.call(generated);
    return generated;
  }

  Future<String?> _createThumbnail(Sticker sticker, Directory target) async {
    final bytes = await File(sticker.filePath).readAsBytes();
    if (_isTruncatedJpeg(bytes)) return null;
    ui.Codec? codec;
    ui.FrameInfo? frame;
    try {
      codec = await ui.instantiateImageCodec(
        bytes,
        targetWidth: 240,
        targetHeight: 240,
      );
      frame = await codec.getNextFrame();
      final data = await frame.image.toByteData(format: ui.ImageByteFormat.png);
      if (data == null) return null;
      final output = File(path.join(target.path, '${sticker.hash}_thumb.png'));
      await output.writeAsBytes(data.buffer.asUint8List());
      return output.path;
    } on Object {
      return null;
    } finally {
      frame?.image.dispose();
      codec?.dispose();
    }
  }

  bool _isTruncatedJpeg(Uint8List bytes) {
    if (bytes.length < 4 ||
        bytes[0] != 0xff ||
        bytes[1] != 0xd8 ||
        bytes[2] != 0xff) {
      return false;
    }
    for (var index = bytes.length - 2; index >= 0; index--) {
      if (bytes[index] == 0xff && bytes[index + 1] == 0xd9) return false;
    }
    return true;
  }
}

class _PreparationOutcome {
  const _PreparationOutcome(this.sticker) : reason = null;
  const _PreparationOutcome.skipped([
    this.reason = _PreparationSkipReason.invalid,
  ]) : sticker = null;

  final Sticker? sticker;
  final _PreparationSkipReason? reason;
}

enum _PreparationSkipReason { invalid, tooLarge, totalLimit }

class _PreparationInput {
  const _PreparationInput(this.inputIndex, this.file, this.source);

  final int inputIndex;
  final File file;
  final StickerSource source;
}

class _ReadFileOutcome {
  const _ReadFileOutcome.data(this.bytes) : reason = null;
  const _ReadFileOutcome.skipped([
    this.reason = _PreparationSkipReason.invalid,
  ]) : bytes = null;

  final Uint8List? bytes;
  final _PreparationSkipReason? reason;
}

class WindowsImportSource implements ImportSource {
  static const int defaultMaxFiles = 1000;

  Future<List<Directory>> discoverCandidates({
    Iterable<String> rememberedDirectories = const <String>[],
  }) async {
    final userProfile = Platform.environment['USERPROFILE'];
    if (userProfile == null || userProfile.isEmpty) return const [];

    final candidates = <Directory>[];
    final seen = <String>{};

    Future<void> addIfExisting(String directoryPath) async {
      final directory = Directory(directoryPath);
      if (!await directory.exists()) return;
      final key = path.normalize(directory.absolute.path).toLowerCase();
      if (seen.add(key)) candidates.add(directory);
    }

    // A directory selected successfully before is the strongest signal. Keep
    // it ahead of broad probes while silently dropping paths that disappeared.
    for (final remembered in rememberedDirectories) {
      if (remembered.trim().isNotEmpty) {
        await addIfExisting(remembered);
      }
    }

    Future<List<Directory>> childDirectories(Directory parent) async {
      if (!await parent.exists()) return const [];
      final result = <Directory>[];
      try {
        await for (final entity
            in parent.list(followLinks: false, recursive: false)) {
          if (entity is Directory) result.add(entity);
        }
      } on Object {
        // A missing or inaccessible account directory should not block import.
      }
      return result;
    }

    final qqAccountName = RegExp(r'^\d+$');

    Future<void> addKnownChildren(
        Directory accountsRoot, List<List<String>> suffixes,
        {bool numericAccountsOnly = false}) async {
      for (final account in await childDirectories(accountsRoot)) {
        if (numericAccountsOnly &&
            !qqAccountName.hasMatch(path.basename(account.path))) {
          continue;
        }
        for (final suffix in suffixes) {
          await addIfExisting(path.joinAll(<String>[account.path, ...suffix]));
        }
      }
    }

    final documentsRoots = <Directory>[
      Directory(path.join(userProfile, 'Documents')),
      Directory(path.join(userProfile, 'OneDrive', 'Documents')),
    ];
    try {
      // Honors a user-moved Windows Documents folder when available.
      final knownDocuments = await getApplicationDocumentsDirectory();
      documentsRoots.add(knownDocuments);
    } on Object {
      // The environment fallbacks above are sufficient when the known-folder
      // API is unavailable (for example, during a test run).
    }

    const qqSuffixes = <List<String>>[
      ['Image', 'Image', 'CustomFace'],
      ['Image', 'CustomFace'],
      ['Image', 'Image', 'CustomFaceRecv'],
      ['Image', 'CustomFaceRecv'],
      ['Cache', 'Image', 'CustomFace'],
      ['CustomFace'],
      ['CustomFaceRecv'],
      // QQNT stores user and marketplace emotions under the account's
      // nt_qq/nt_data tree. These folders may contain extensionless images.
      ['nt_qq', 'nt_data', 'Emoji', 'personal_emoji', 'Ori'],
      ['nt_qq', 'nt_data', 'Emoji', 'marketface'],
      ['nt_qq', 'nt_data', 'Emoji', 'emoji-recv'],
    ];
    const wechatSuffixes = <List<String>>[
      ['FileStorage', 'CustomEmotion'],
      ['FileStorage', 'CustomEmotions'],
    ];

    for (final documents in documentsRoots) {
      await addKnownChildren(
          Directory(path.join(documents.path, 'Tencent Files')), qqSuffixes,
          numericAccountsOnly: true);
      for (final wechatRootName in ['WeChat Files', 'xwechat_files']) {
        await addKnownChildren(
            Directory(path.join(documents.path, wechatRootName)),
            wechatSuffixes);
      }
    }

    // Older QQ desktop builds stored account data below the install directory.
    // Only named custom-face folders are considered; the install root itself is
    // never recursively scanned because it also contains chat media and cache.
    final installRoots = <String?>[
      Platform.environment['ProgramFiles'],
      Platform.environment['ProgramFiles(x86)'],
    ];
    for (final installRoot in installRoots) {
      if (installRoot == null || installRoot.isEmpty) continue;
      for (final qqRoot in [
        Directory(path.join(installRoot, 'Tencent', 'QQ')),
        Directory(path.join(installRoot, 'Tencent', 'QQ', 'Users')),
      ]) {
        await addKnownChildren(qqRoot, qqSuffixes, numericAccountsOnly: true);
      }
    }

    // QQNT portable/custom installs can keep the account data beside the
    // executable. Probe known layouts on each drive without recursively
    // scanning any drive or QQ installation directory.
    const portableDataLayouts = <List<String>>[
      ['QQ_NT', 'dialogue', 'Tencent Files'],
      ['QQNT', 'dialogue', 'Tencent Files'],
    ];
    for (var drive = 2; drive < 26; drive++) {
      final driveRoot =
          '${String.fromCharCode('A'.codeUnitAt(0) + drive)}:${Platform.pathSeparator}';
      for (final layout in portableDataLayouts) {
        await addKnownChildren(
            Directory(path.joinAll(<String>[driveRoot, ...layout])), qqSuffixes,
            numericAccountsOnly: true);
      }

      // A common portable layout is `QQ\\<version-or-channel>\\dialogue\\Tencent Files`.
      // Discover the intermediate directory instead of assuming a drive or
      // version name, then only accept numeric account folders below it.
      final qqRoot = Directory(path.join(driveRoot, 'QQ'));
      await addKnownChildren(
          Directory(path.join(qqRoot.path, 'dialogue', 'Tencent Files')),
          qqSuffixes,
          numericAccountsOnly: true);
      for (final installDirectory in await childDirectories(qqRoot)) {
        await addKnownChildren(
            Directory(
                path.join(installDirectory.path, 'dialogue', 'Tencent Files')),
            qqSuffixes,
            numericAccountsOnly: true);
      }
    }

    // Some installations move these data roots out of Documents. They are
    // still handled only when an account directory contains a known folder.
    final appData = Platform.environment['APPDATA'];
    if (appData != null && appData.isNotEmpty) {
      await addKnownChildren(
          Directory(path.join(appData, 'Tencent', 'Files')), qqSuffixes,
          numericAccountsOnly: true);
      for (final wechatRootName in ['WeChat Files', 'xwechat_files']) {
        await addKnownChildren(
            Directory(path.join(appData, 'Tencent', wechatRootName)),
            wechatSuffixes);
      }
    }

    return candidates;
  }

  @override
  Future<List<File>> scan(Directory root,
      {int maxFiles = defaultMaxFiles}) async {
    final discovered = <_ScannedFile>[];
    final restrictToEmotionFolders = _isQqDataRoot(root.path);
    try {
      await for (final entity
          in root.list(recursive: true, followLinks: false)) {
        if (discovered.length >= maxFiles) break;
        if (entity is! File) continue;
        final isEmotionPath = _isQqNtEmojiPath(entity.path);
        if (restrictToEmotionFolders && !isEmotionPath) continue;
        // Use the file signature as the source of truth. QQNT can omit
        // extensions, and manually selected folders may contain images with
        // an arbitrary extension. Automatic discovery is still constrained to
        // known emotion folders before this check runs.
        if (await _hasImageSignature(entity)) {
          FileStat? stat;
          try {
            stat = await entity.stat();
          } on Object {
            stat = null;
          }
          discovered.add(_ScannedFile(entity, stat));
        }
      }
    } on Object {
      // A locked or partially removed source folder is treated as empty.
    }
    if (sourceFor(root) == StickerSource.qq) {
      // QQNT's Ori filenames are content hashes and its directory enumeration
      // is lexical, so neither can represent the visible collection order.
      // Windows preserves the original file creation time; use it as the
      // stable order signal and fall back to the path for same-timestamp files.
      final epoch = DateTime.fromMillisecondsSinceEpoch(0);
      discovered.sort((a, b) {
        final changed =
            (a.stat?.changed ?? epoch).compareTo(b.stat?.changed ?? epoch);
        if (changed != 0) return changed;
        final modified =
            (a.stat?.modified ?? epoch).compareTo(b.stat?.modified ?? epoch);
        if (modified != 0) return modified;
        return a.file.path.toLowerCase().compareTo(b.file.path.toLowerCase());
      });
    }
    return discovered.map((entry) => entry.file).toList(growable: false);
  }

  bool _isQqNtEmojiPath(String filePath) {
    final value = filePath.toLowerCase().replaceAll('/', '\\');
    return value.contains('\\personal_emoji\\') ||
        value.contains('\\marketface') ||
        value.contains('\\emoji-recv') ||
        RegExp(r'\\customface(?:recv)?(?:\\|$)').hasMatch(value);
  }

  bool _isQqDataRoot(String directoryPath) {
    final value = directoryPath.toLowerCase().replaceAll('/', '\\');
    return RegExp(r'\\tencent files(?:\\\d+)?(?:\\|$)').hasMatch(value) &&
        !_isQqNtEmojiPath(value);
  }

  Future<bool> _hasImageSignature(File file) async {
    RandomAccessFile? handle;
    try {
      handle = await file.open();
      final bytes = await handle.read(16);
      if (bytes.length >= 8 &&
          bytes[0] == 0x89 &&
          bytes[1] == 0x50 &&
          bytes[2] == 0x4e &&
          bytes[3] == 0x47 &&
          bytes[4] == 0x0d &&
          bytes[5] == 0x0a &&
          bytes[6] == 0x1a &&
          bytes[7] == 0x0a) {
        return true;
      }
      if (bytes.length >= 6) {
        final gif = String.fromCharCodes(bytes.take(6));
        if (gif == 'GIF87a' || gif == 'GIF89a') return true;
      }
      if (bytes.length >= 3 &&
          bytes[0] == 0xff &&
          bytes[1] == 0xd8 &&
          bytes[2] == 0xff) {
        return true;
      }
      if (bytes.length >= 12 &&
          String.fromCharCodes(bytes.sublist(0, 4)) == 'RIFF' &&
          String.fromCharCodes(bytes.sublist(8, 12)) == 'WEBP') {
        return true;
      }
      return bytes.length >= 2 && bytes[0] == 0x42 && bytes[1] == 0x4d;
    } on Object {
      return false;
    } finally {
      await handle?.close();
    }
  }

  @override
  StickerSource sourceFor(Directory root) {
    final value = root.path.toLowerCase().replaceAll('/', '\\');
    if (RegExp(r'\\(?:wechat files|xwechat_files)(?:\\|$)').hasMatch(value)) {
      return StickerSource.wechat;
    }
    final isQqEmotionPath = value.contains('\\personal_emoji\\') ||
        value.contains('\\marketface') ||
        value.contains('\\emoji-recv') ||
        RegExp(r'\\customface(?:recv)?(?:\\|$)').hasMatch(value) ||
        RegExp(r'\\tencent files(?:\\\d+)?(?:\\|$)').hasMatch(value);
    if (isQqEmotionPath) {
      return StickerSource.qq;
    }
    return StickerSource.manual;
  }
}

class _ScannedFile {
  const _ScannedFile(this.file, this.stat);

  final File file;
  final FileStat? stat;
}
