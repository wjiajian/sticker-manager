import 'dart:io';
import 'package:flutter/material.dart';
import '../models.dart';
import 'app_theme.dart';

class RecentStickers extends StatefulWidget {
  const RecentStickers(
      {super.key,
      required this.entries,
      required this.onUse,
      required this.onCopy});
  final List<RankedSticker> entries;
  final ValueChanged<RankedSticker> onUse;
  final ValueChanged<RankedSticker> onCopy;

  @override
  State<RecentStickers> createState() => _RecentStickersState();
}

class _RecentStickersState extends State<RecentStickers> {
  final _scroll = ScrollController();
  bool _canBack = false;
  bool _canForward = false;

  void _updateScroll() {
    if (!mounted || !_scroll.hasClients) return;
    final back = _scroll.position.extentBefore > 1;
    final forward = _scroll.position.extentAfter > 1;
    if (back != _canBack || forward != _canForward) {
      setState(() {
        _canBack = back;
        _canForward = forward;
      });
    }
  }

  @override
  void initState() {
    super.initState();
    _scroll.addListener(_updateScroll);
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  void _page(int direction) {
    if (!_scroll.hasClients) return;
    _scroll.animateTo(
        (_scroll.offset + direction * _scroll.position.viewportDimension)
            .clamp(0.0, _scroll.position.maxScrollExtent),
        duration: const Duration(milliseconds: 180),
        curve: Curves.easeOut);
  }

  @override
  Widget build(BuildContext context) {
    final platform = Theme.of(context).platform;
    final desktop =
        platform == TargetPlatform.windows || platform == TargetPlatform.macOS;
    final size = AppTheme.thumbnailSize(platform) + 8;
    WidgetsBinding.instance.addPostFrameCallback((_) => _updateScroll());
    return Padding(
      padding: const EdgeInsets.fromLTRB(20, 12, 20, 4),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Row(children: [
          const Text('最近使用',
              style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.secondaryText)),
          const SizedBox(width: 8),
          Text('${widget.entries.length}',
              style:
                  const TextStyle(fontSize: 12, color: AppTheme.secondaryText)),
          if (desktop && (_canBack || _canForward)) ...[
            const SizedBox(width: 12),
            IconButton(
                tooltip: '上一页最近使用',
                onPressed: _canBack ? () => _page(-1) : null,
                visualDensity: VisualDensity.compact,
                icon: const Icon(Icons.chevron_left, size: 18)),
            IconButton(
                tooltip: '下一页最近使用',
                onPressed: _canForward ? () => _page(1) : null,
                visualDensity: VisualDensity.compact,
                icon: const Icon(Icons.chevron_right, size: 18)),
          ],
        ]),
        const SizedBox(height: 8),
        SizedBox(
            height: size + 8,
            child: Row(children: [
              Expanded(
                  child: NotificationListener<ScrollMetricsNotification>(
                      onNotification: (_) {
                        WidgetsBinding.instance
                            .addPostFrameCallback((_) => _updateScroll());
                        return false;
                      },
                      child: ListView.separated(
                        controller: _scroll,
                        scrollDirection: Axis.horizontal,
                        itemCount: widget.entries.length,
                        separatorBuilder: (_, __) => const SizedBox(width: 8),
                        itemBuilder: (context, index) {
                          final entry = widget.entries[index];
                          return Tooltip(
                              message: entry.sticker.note,
                              waitDuration: const Duration(milliseconds: 500),
                              constraints: const BoxConstraints(maxWidth: 320),
                              child: GestureDetector(
                                onSecondaryTapDown: (details) async {
                                  final overlay = Overlay.of(context)
                                      .context
                                      .findRenderObject() as RenderBox;
                                  final selected = await showMenu<bool>(
                                      context: context,
                                      position: RelativeRect.fromRect(
                                          details.globalPosition &
                                              const Size(1, 1),
                                          Offset.zero & overlay.size),
                                      items: const [
                                        PopupMenuItem(
                                            value: true, child: Text('复制'))
                                      ]);
                                  if (selected == true) widget.onCopy(entry);
                                },
                                child: Material(
                                  color: AppTheme.cardBackground,
                                  shape: RoundedRectangleBorder(
                                      borderRadius: BorderRadius.circular(
                                          AppTheme.cardRadius),
                                      side: const BorderSide(
                                          color: AppTheme.border)),
                                  clipBehavior: Clip.antiAlias,
                                  child: InkWell(
                                    borderRadius: BorderRadius.circular(
                                        AppTheme.cardRadius),
                                    onTap: () => widget.onUse(entry),
                                    child: Padding(
                                      padding: const EdgeInsets.all(4),
                                      child: Image.file(
                                          File(entry
                                                  .sticker.thumbnailPath.isEmpty
                                              ? entry.sticker.filePath
                                              : entry.sticker.thumbnailPath),
                                          width: size,
                                          height: size,
                                          fit: BoxFit.contain,
                                          errorBuilder: (_, __, ___) =>
                                              SizedBox(
                                                  width: size,
                                                  child: const Icon(Icons
                                                      .broken_image_outlined))),
                                    ),
                                  ),
                                ),
                              ));
                        },
                      ))),
            ])),
      ]),
    );
  }
}
