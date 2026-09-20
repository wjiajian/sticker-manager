import 'dart:io';
import '../models.dart';

class ImportCandidate {
  const ImportCandidate(
      {required this.file,
      required this.source,
      required this.bytes,
      this.hash,
      this.mediaType,
      this.existing,
      this.error});
  final File file;
  final StickerSource source;
  final int bytes;
  final String? hash;
  final StickerMediaType? mediaType;
  final RankedSticker? existing;
  final String? error;
  bool get valid => error == null && hash != null;
}

class ImportDecision {
  const ImportDecision(this.candidates, this.groupId);
  final List<ImportCandidate> candidates;
  final String groupId;
}
