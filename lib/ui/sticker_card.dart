import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';

import '../models.dart';
import '../platform/desktop_platform.dart';
import '../services/database.dart';
import 'app_theme.dart';

/// A single sticker tile: white card, image at its original aspect ratio,
/// one-line note, hover copy button and selection/focus chrome.
///
/// A single tap is delayed briefly so a double tap can collapse two clicks
/// into one use action, and a pending tap is cancelled when the card is
/// reused for another sticker or the parent switches interaction modes.
class StickerCard extends StatefulWidget {
  const StickerCard({
    super.key,
    required this.entry,
    required this.selectionMode,
    required this.selected,
    required this.keyboardFocused,
    required this.onUse,
    required this.onSelect,
    required this.onCopy,
    required this.onLongPress,
    required this.onPin,
    required this.onGroups,
    required this.onEdit,
    required this.onDelete,
    required this.onContextMenu,
    required this.compact,
  });

  final RankedSticker entry;
  final bool selectionMode;
  final bool selected;
  final bool keyboardFocused;
  final VoidCallback onUse;
  final VoidCallback onSelect;
  final VoidCallback onCopy;
  final VoidCallback? onLongPress;
  final VoidCallback onPin;
  final VoidCallback onGroups;
  final VoidCallback onEdit;
  final VoidCallback onDelete;
  final Future<void> Function(Offset globalPosition) onContextMenu;
  final bool compact;

  @override
  State<StickerCard> createState() => _StickerCardState();
}

class _StickerCardState extends State<StickerCard> {
  Timer? _tapTimer;
  DateTime? _lastActionAt;
  bool _hovering = false;

  static const _singleTapDelay = Duration(milliseconds: 260);
  static const _actionDedupWindow = Duration(milliseconds: 450);

  @override
  void dispose() {
    _tapTimer?.cancel();
    super.dispose();
  }

  @override
  void didUpdateWidget(covariant StickerCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    // A pending tap belongs to the card and interaction mode that received
    // it. Cancel it if Flutter ever reuses this State for another item or if
    // the parent switches between send and selection modes before the delay
    // expires.
    if (oldWidget.entry.sticker.id != widget.entry.sticker.id ||
        oldWidget.selectionMode != widget.selectionMode) {
      _tapTimer?.cancel();
      _tapTimer = null;
      _lastActionAt = null;
    }
  }

  void _handleTap() {
    _tapTimer?.cancel();
    _tapTimer = Timer(_singleTapDelay, _dispatchAction);
  }

  void _handleDoubleTap() {
    _tapTimer?.cancel();
    _dispatchAction();
  }

  void _dispatchAction() {
    final now = DateTime.now();
    final lastActionAt = _lastActionAt;
    if (lastActionAt != null &&
        now.difference(lastActionAt) < _actionDedupWindow) {
      return;
    }
    _lastActionAt = now;
    if (widget.selectionMode) {
      widget.onSelect();
    } else {
      widget.onUse();
    }
  }

  Color get _borderColor {
    if (widget.selected) return AppTheme.accent;
    if (_hovering || widget.keyboardFocused) return AppTheme.hoverBorder;
    return AppTheme.border;
  }

  @override
  Widget build(BuildContext context) {
    final entry = widget.entry;
    final sticker = entry.sticker;
    final note = sticker.note;
    final showCopyButton =
        isDesktopPlatform && !widget.selectionMode && _hovering;
    return MouseRegion(
      onEnter: (_) {
        if (mounted) setState(() => _hovering = true);
      },
      onExit: (_) {
        if (mounted) setState(() => _hovering = false);
      },
      child: Container(
        decoration: BoxDecoration(
          color: AppTheme.cardBackground,
          borderRadius: BorderRadius.circular(AppTheme.cardRadius),
          border: Border.all(
            color: _borderColor,
            width: widget.selected ? 1.5 : 1,
          ),
          boxShadow: _hovering
              ? const [
                  BoxShadow(
                    color: Color(0x10008B8B),
                    blurRadius: 5,
                    offset: Offset(0, 1),
                  ),
                ]
              : null,
        ),
        foregroundDecoration: widget.keyboardFocused
            ? BoxDecoration(
                borderRadius: BorderRadius.circular(AppTheme.cardRadius),
                border: Border.all(
                  color:
                      widget.selected ? AppTheme.primaryText : AppTheme.accent,
                  width: 2,
                ),
              )
            : null,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(AppTheme.cardRadius - 1),
          child: InkWell(
            onTap: _handleTap,
            onDoubleTap: _handleDoubleTap,
            onLongPress: widget.onLongPress,
            onSecondaryTapDown: isDesktopPlatform
                ? (details) => widget.onContextMenu(details.globalPosition)
                : null,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(
                  child: Stack(
                    children: [
                      Positioned.fill(
                        child: Padding(
                          padding: const EdgeInsets.fromLTRB(8, 8, 8, 4),
                          child: _StickerImage(
                            sticker: sticker,
                            animateGif: _hovering,
                          ),
                        ),
                      ),
                      if (widget.selectionMode)
                        Positioned(
                          left: 6,
                          top: 6,
                          child: _SelectionIndicator(
                            selected: widget.selected,
                            onTap: widget.onSelect,
                          ),
                        ),
                      if (showCopyButton)
                        Positioned(
                          right: 8,
                          top: 8,
                          child: _CopyButton(onPressed: widget.onCopy),
                        )
                      else if (isDesktopPlatform && sticker.isPinned)
                        const Positioned(
                          right: 8,
                          top: 8,
                          child: Tooltip(
                            message: '已置顶',
                            child: Icon(Icons.push_pin,
                                size: 18, color: AppTheme.secondaryText),
                          ),
                        ),
                      if (!isDesktopPlatform)
                        Positioned(
                          right: 4,
                          top: 4,
                          child: IconButton.filledTonal(
                              onPressed: widget.onPin,
                              icon: Icon(sticker.isPinned
                                  ? Icons.push_pin
                                  : Icons.push_pin_outlined),
                              tooltip: sticker.isPinned ? '取消置顶' : '置顶'),
                        ),
                    ],
                  ),
                ),
                if (!(widget.compact && isDesktopPlatform))
                  Padding(
                    padding: EdgeInsets.fromLTRB(
                        16, 6, isDesktopPlatform ? 12 : 6, 12),
                    child: isDesktopPlatform
                        ? _NoteLabel(note: note)
                        : Row(
                            children: [
                              Expanded(
                                  child: Text(note.isEmpty ? '未备注' : note,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis)),
                              IconButton(
                                  onPressed: widget.onGroups,
                                  icon: const Icon(Icons.folder_copy_outlined,
                                      size: 18),
                                  tooltip: '管理分组'),
                              IconButton(
                                  onPressed: widget.onEdit,
                                  icon:
                                      const Icon(Icons.edit_outlined, size: 18),
                                  tooltip: '编辑备注'),
                              IconButton(
                                  onPressed: widget.onDelete,
                                  icon: const Icon(Icons.delete_outline,
                                      size: 18),
                                  tooltip: '删除表情'),
                            ],
                          ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// One-line note label; hovering shows the complete note in a tooltip.
class _NoteLabel extends StatelessWidget {
  const _NoteLabel({required this.note});

  final String note;

  @override
  Widget build(BuildContext context) {
    final text = Text(
      note.isEmpty ? '未备注' : note,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: TextStyle(
        fontSize: 17,
        height: 1.2,
        color: note.isEmpty ? AppTheme.secondaryText : AppTheme.primaryText,
      ),
    );
    if (note.isEmpty) return text;
    return Tooltip(message: note, child: text);
  }
}

class _SelectionIndicator extends StatelessWidget {
  const _SelectionIndicator({required this.selected, required this.onTap});

  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: onTap,
      child: Container(
        width: 22,
        height: 22,
        decoration: BoxDecoration(
          color: selected ? AppTheme.accent : AppTheme.cardBackground,
          borderRadius: BorderRadius.circular(6),
          border: Border.all(
            color: selected ? AppTheme.accent : AppTheme.secondaryText,
            width: 1.5,
          ),
        ),
        child: selected
            ? const Icon(Icons.check, size: 16, color: Colors.white)
            : null,
      ),
    );
  }
}

/// Clipboard-only action shown on hover. It is stacked above the card's
/// InkWell, so tapping it never dispatches the card's use action.
class _CopyButton extends StatelessWidget {
  const _CopyButton({required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: AppTheme.cardBackground,
        borderRadius: BorderRadius.circular(9),
        border: Border.all(color: AppTheme.border),
        boxShadow: const [
          BoxShadow(
            color: Color(0x14000000),
            blurRadius: 4,
            offset: Offset(0, 1),
          ),
        ],
      ),
      child: IconButton(
        onPressed: onPressed,
        tooltip: '复制',
        icon: const Icon(Icons.copy_outlined,
            size: 21, color: AppTheme.primaryText),
        iconSize: 21,
        padding: EdgeInsets.zero,
        constraints: const BoxConstraints.tightFor(width: 36, height: 36),
        visualDensity: VisualDensity.compact,
      ),
    );
  }
}

class _StickerImage extends StatelessWidget {
  const _StickerImage({required this.sticker, this.animateGif = false});

  final Sticker sticker;
  final bool animateGif;

  @override
  Widget build(BuildContext context) {
    const placeholder = Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.broken_image_outlined),
          SizedBox(height: 4),
          Text('无法预览', style: TextStyle(fontSize: 12)),
        ],
      ),
    );
    final fallback = Image.file(
      File(sticker.filePath),
      key: ValueKey('${sticker.id}-source'),
      fit: BoxFit.contain,
      cacheWidth: 240,
      cacheHeight: 240,
      filterQuality: FilterQuality.low,
      errorBuilder: (_, __, ___) => placeholder,
    );
    if (animateGif && sticker.mediaType == StickerMediaType.gif) {
      return Image.file(
        File(sticker.filePath),
        key: ValueKey('${sticker.id}-animated'),
        fit: BoxFit.contain,
        cacheWidth: 240,
        cacheHeight: 240,
        filterQuality: FilterQuality.low,
        gaplessPlayback: true,
        errorBuilder: (_, __, ___) => fallback,
      );
    }
    if (sticker.thumbnailPath.isEmpty ||
        sticker.thumbnailVersion < StickerDatabase.currentThumbnailVersion) {
      return fallback;
    }
    return Image.file(
      File(sticker.thumbnailPath),
      key: ValueKey('${sticker.id}-${sticker.thumbnailVersion}'),
      fit: BoxFit.contain,
      cacheWidth: 240,
      cacheHeight: 240,
      filterQuality: FilterQuality.low,
      errorBuilder: (_, __, ___) => fallback,
    );
  }
}
