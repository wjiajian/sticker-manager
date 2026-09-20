import 'package:flutter/material.dart';
import '../models.dart';
import '../services/repository.dart';

class AddToGroupsDialog extends StatefulWidget {
  const AddToGroupsDialog(
      {super.key,
      required this.repository,
      required this.entries,
      required this.groups});
  final StickerRepository repository;
  final List<RankedSticker> entries;
  final List<StickerGroup> groups;
  @override
  State<AddToGroupsDialog> createState() => _AddToGroupsDialogState();
}

class _AddToGroupsDialogState extends State<AddToGroupsDialog> {
  late final _groups = widget.groups.where((g) => g.id != 'all').toList();
  final _selected = <String>{};
  String _query = '';
  final _search = TextEditingController();
  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  String? _error;
  bool _busy = false;

  Future<void> _create() async {
    String name = '';
    final result = await showDialog<String>(
        context: context,
        builder: (context) => AlertDialog(
              title: const Text('新建分组'),
              content: TextField(
                  autofocus: true,
                  onChanged: (value) => name = value.trim(),
                  decoration: const InputDecoration(labelText: '分组名称')),
              actions: [
                TextButton(
                    onPressed: () => Navigator.pop(context),
                    child: const Text('取消')),
                FilledButton(
                    onPressed: () {
                      if (name.isNotEmpty) Navigator.pop(context, name);
                    },
                    child: const Text('创建'))
              ],
            ));
    if (result == null || !mounted) return;
    setState(() => _busy = true);
    try {
      final id = DateTime.now().microsecondsSinceEpoch.toString();
      await widget.repository.createGroup(id, result);
      if (!mounted) return;
      setState(() {
        _groups
            .add(StickerGroup(id: id, name: result, createdAt: DateTime.now()));
        _selected.add(id);
        _query = '';
        _search.clear();
      });
    } on Object catch (error) {
      if (mounted) setState(() => _error = '创建失败：$error');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
        title: Text('将 ${widget.entries.length} 张表情添加到分组'),
        content: SizedBox(
            width: 420,
            child: Column(mainAxisSize: MainAxisSize.min, children: [
              const Text('保留原分组，增加所选分组。'),
              TextField(
                  controller: _search,
                  onChanged: (value) =>
                      setState(() => _query = value.trim().toLowerCase()),
                  decoration: const InputDecoration(labelText: '搜索分组')),
              const SizedBox(height: 8),
              Flexible(
                  child: SingleChildScrollView(
                      child: Column(children: [
                for (final group in _groups
                    .where((g) => g.name.toLowerCase().contains(_query)))
                  Builder(builder: (context) {
                    final count = widget.entries
                        .where((e) => e.groupIds.contains(group.id))
                        .length;
                    final all = count == widget.entries.length;
                    return CheckboxListTile(
                        title: Text(group.name),
                        subtitle: Text(all
                            ? '已全部添加'
                            : '已有 $count/${widget.entries.length} 张'),
                        value: all || _selected.contains(group.id),
                        onChanged: all || _busy
                            ? null
                            : (value) => setState(() {
                                  if (value == true) {
                                    _selected.add(group.id);
                                  } else {
                                    _selected.remove(group.id);
                                  }
                                }));
                  }),
              ]))),
              if (_error != null) Text(_error!),
              TextButton.icon(
                  onPressed: _busy ? null : _create,
                  icon: const Icon(Icons.add),
                  label: const Text('新建分组')),
            ])),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(context), child: const Text('取消')),
          FilledButton(
              onPressed: _busy || _selected.isEmpty
                  ? null
                  : () => Navigator.pop(context, _selected),
              child: Text('添加到 ${_selected.length} 个分组'))
        ],
      );
}
