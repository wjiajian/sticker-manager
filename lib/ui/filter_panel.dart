import 'package:flutter/material.dart';

import '../models.dart';
import '../services/sticker_filter.dart';

/// Owns a draft; dismissing the route never changes the applied criteria.
class FilterPanel extends StatefulWidget {
  const FilterPanel({super.key, required this.initial});

  final StickerFilter initial;

  @override
  State<FilterPanel> createState() => _FilterPanelState();
}

class _FilterPanelState extends State<FilterPanel> {
  StickerMediaType? _mediaType;
  Set<StickerSource> _sources = {};
  ImportTimeRange _timeRange = ImportTimeRange.any;
  DateTimeRange? _dates;
  bool _pinnedOnly = false;

  @override
  void initState() {
    super.initState();
    _setDraft(widget.initial);
  }

  void _setDraft(StickerFilter value) {
    _mediaType = value.mediaType;
    _sources = {...value.sources};
    _timeRange = value.timeRange;
    _dates = value.startDate == null || value.endDate == null
        ? null
        : DateTimeRange(start: value.startDate!, end: value.endDate!);
    _pinnedOnly = value.pinnedOnly;
  }

  Future<void> _chooseDates() async {
    final now = DateTime.now();
    final dates = await showDateRangePicker(
      context: context,
      initialDateRange: _dates,
      firstDate: DateTime(1970),
      lastDate: DateTime(now.year + 1, 12, 31),
      helpText: '选择导入日期',
      cancelText: '取消',
      confirmText: '确定',
      saveText: '确定',
    );
    if (!mounted || dates == null) return;
    setState(() {
      _dates = dates;
      _timeRange = ImportTimeRange.custom;
    });
  }

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                const Expanded(
                  child: Text(
                    '筛选表情',
                    style: TextStyle(fontSize: 20, fontWeight: FontWeight.w600),
                  ),
                ),
                IconButton(
                  tooltip: '关闭筛选',
                  onPressed: () => Navigator.pop(context),
                  icon: const Icon(Icons.close),
                ),
              ],
            ),
            Flexible(
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    const Text('类型'),
                    Wrap(
                      spacing: 8,
                      children: [
                        for (final type in [null, ...StickerMediaType.values])
                          ChoiceChip(
                            label: Text(
                              type == null
                                  ? '全部'
                                  : type == StickerMediaType.gif
                                      ? 'GIF'
                                      : '静态图',
                            ),
                            selected: _mediaType == type,
                            onSelected: (_) =>
                                setState(() => _mediaType = type),
                          ),
                      ],
                    ),
                    const SizedBox(height: 12),
                    const Text('来源（未选择表示全部）'),
                    Wrap(
                      spacing: 8,
                      children: [
                        for (final source in StickerSource.values)
                          FilterChip(
                            label: Text(StickerFilter.sourceLabel(source)),
                            selected: _sources.contains(source),
                            onSelected: (selected) => setState(() {
                              if (selected) {
                                _sources.add(source);
                              } else {
                                _sources.remove(source);
                              }
                            }),
                          ),
                      ],
                    ),
                    const SizedBox(height: 12),
                    DropdownButtonFormField<ImportTimeRange>(
                      key: ValueKey(_timeRange),
                      initialValue: _timeRange,
                      decoration: const InputDecoration(labelText: '导入时间'),
                      items: [
                        for (final range in ImportTimeRange.values)
                          DropdownMenuItem(
                            value: range,
                            child: Text(StickerFilter.timeLabel(range)),
                          ),
                      ],
                      onChanged: (value) {
                        if (value == null) return;
                        setState(() => _timeRange = value);
                        if (value == ImportTimeRange.custom) _chooseDates();
                      },
                    ),
                    if (_timeRange == ImportTimeRange.custom)
                      TextButton(
                        onPressed: _chooseDates,
                        child: Text(
                          _dates == null
                              ? '选择日期范围'
                              : '${StickerFilter.dateLabel(_dates!.start)}—${StickerFilter.dateLabel(_dates!.end)}',
                        ),
                      ),
                    const SizedBox(height: 12),
                    const Text('置顶'),
                    Wrap(
                      spacing: 8,
                      children: [
                        for (final pinned in [false, true])
                          ChoiceChip(
                            label: Text(pinned ? '仅置顶' : '全部'),
                            selected: _pinnedOnly == pinned,
                            onSelected: (_) =>
                                setState(() => _pinnedOnly = pinned),
                          ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: () =>
                      setState(() => _setDraft(const StickerFilter())),
                  child: const Text('重置'),
                ),
                const SizedBox(width: 8),
                FilledButton(
                  onPressed:
                      _timeRange == ImportTimeRange.custom && _dates == null
                          ? null
                          : () => Navigator.pop(
                                context,
                                StickerFilter(
                                  mediaType: _mediaType,
                                  sources: Set.unmodifiable(_sources),
                                  timeRange: _timeRange,
                                  startDate: _dates?.start,
                                  endDate: _dates?.end,
                                  pinnedOnly: _pinnedOnly,
                                ),
                              ),
                  child: const Text('应用'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
