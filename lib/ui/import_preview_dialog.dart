import 'dart:io';
import 'package:flutter/material.dart';
import 'package:path/path.dart' as path;
import '../models.dart';
import '../services/import_preview.dart';
import '../services/import_source.dart';
import '../services/sticker_filter.dart';

class ImportPreviewDialog extends StatefulWidget {
  const ImportPreviewDialog(
      {super.key,
      required this.title,
      required this.candidates,
      required this.groups,
      required this.initialGroup});
  final String title;
  final List<ImportCandidate> candidates;
  final List<StickerGroup> groups;
  final String initialGroup;
  @override
  State<ImportPreviewDialog> createState() => _ImportPreviewDialogState();
}

class _ImportPreviewDialogState extends State<ImportPreviewDialog> {
  late String _group = widget.initialGroup;
  late final _selected = <int>{
    for (var i = 0; i < widget.candidates.length; i++)
      if (widget.candidates[i].valid &&
          defaultSelectImportFile(widget.candidates[i].file))
        i
  };
  @override
  Widget build(BuildContext context) {
    final hashes = <String>{};
    var added = 0, grouped = 0, exists = 0;
    final statuses = <int, String>{};
    for (final i in _selected.toList()..sort()) {
      final c = widget.candidates[i];
      if (!hashes.add(c.hash!)) {
        exists++;
        statuses[i] = '已存在（本批次重复）';
      } else if (c.existing == null) {
        added++;
        statuses[i] = '新增图片';
      } else if (!c.existing!.groupIds.contains(_group) ||
          (c.source == StickerSource.qq &&
              !c.existing!.groupIds.contains('qq_favorites'))) {
        grouped++;
        statuses[i] = '已有图片加入分组';
      } else {
        exists++;
        statuses[i] = '已存在';
      }
    }
    final skipped = widget.candidates.where((c) => !c.valid).length;
    return AlertDialog(
        title: Text(widget.title),
        content: SizedBox(
            width: 580,
            child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  DropdownButtonFormField<String>(
                      initialValue: _group,
                      decoration: const InputDecoration(labelText: '目标分组'),
                      items: widget.groups
                          .map((g) => DropdownMenuItem(
                              value: g.id,
                              child: Text(g.id == 'all' ? '全部表情' : g.name)))
                          .toList(),
                      onChanged: (value) {
                        if (value != null) setState(() => _group = value);
                      }),
                  const SizedBox(height: 8),
                  Text(
                      '新增图片 $added · 已有图片加入分组 $grouped · 已存在 $exists · 跳过 $skipped'),
                  const Text(
                      '每批最多 512 MiB，单文件最多 64 MiB。目录沿用现有扫描范围及每目录 1000 个文件上限。',
                      style: TextStyle(fontSize: 12)),
                  Row(children: [
                    TextButton(
                        onPressed: () => setState(() {
                              _selected.addAll([
                                for (var i = 0;
                                    i < widget.candidates.length;
                                    i++)
                                  if (widget.candidates[i].valid) i
                              ]);
                            }),
                        child: const Text('全选')),
                    TextButton(
                        onPressed: () => setState(_selected.clear),
                        child: const Text('全不选'))
                  ]),
                  Flexible(
                      child: SizedBox(
                          height: 340,
                          child: ListView.builder(
                              itemCount: widget.candidates.length,
                              itemBuilder: (context, i) {
                                final c = widget.candidates[i];
                                return CheckboxListTile(
                                    value: _selected.contains(i),
                                    onChanged: !c.valid
                                        ? null
                                        : (value) => setState(() {
                                              if (value == true) {
                                                _selected.add(i);
                                              } else {
                                                _selected.remove(i);
                                              }
                                            }),
                                    secondary: c.valid
                                        ? Image.file(File(c.file.path),
                                            width: 48,
                                            height: 48,
                                            fit: BoxFit.contain,
                                            cacheWidth: 96,
                                            errorBuilder: (_, __, ___) =>
                                                const Icon(Icons
                                                    .broken_image_outlined))
                                        : const Icon(Icons.warning_amber),
                                    title: Text(path.basename(c.file.path),
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis),
                                    subtitle: Text(
                                        '${StickerFilter.sourceLabel(c.source)} · ${c.error ?? statuses[i] ?? (defaultSelectImportFile(c.file) ? '未选择' : '群聊或市场目录，默认不导入')}'));
                              }))),
                ])),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(context), child: const Text('取消')),
          FilledButton(
              onPressed: _selected.isEmpty
                  ? null
                  : () => Navigator.pop(
                      context,
                      ImportDecision([
                        for (final i in _selected.toList()..sort())
                          widget.candidates[i]
                      ], _group)),
              child: const Text('开始导入'))
        ]);
  }
}
