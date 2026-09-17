import 'dart:ui' as ui;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../models.dart';
import '../platform/desktop_platform.dart';
import 'app_theme.dart';
import 'grid_metrics.dart';
import 'sticker_card.dart';

/// Sticker grid with scrolling, drag (rubber-band) selection and hit testing.
/// All geometry comes from [GridMetrics]; the resolved metrics are reported
/// to the parent via [onMetricsChanged] so keyboard navigation and scrolling
/// can reuse the same positions.
class StickerGrid extends StatefulWidget {
  const StickerGrid({
    super.key,
    required this.stickers,
    required this.density,
    required this.quickPicker,
    required this.selectionMode,
    required this.selectedIds,
    required this.focusedIndex,
    this.showKeyboardFocus = true,
    required this.scrollController,
    required this.onMetricsChanged,
    required this.onUse,
    required this.onSelect,
    required this.onDragSelection,
    required this.onExitSelection,
    required this.onCopy,
    required this.onLongPress,
    required this.onPin,
    required this.onGroups,
    required this.onEdit,
    required this.onDelete,
    required this.onContextMenu,
  });

  final List<RankedSticker> stickers;
  final GridDensity density;
  final bool quickPicker;
  final bool selectionMode;
  final Set<String> selectedIds;
  final int focusedIndex;
  final bool showKeyboardFocus;
  final ScrollController scrollController;
  final ValueChanged<GridMetrics> onMetricsChanged;
  final ValueChanged<RankedSticker> onUse;

  /// Toggles one sticker in the selection. The grid intercepts the tap that
  /// immediately follows a drag selection, so it never reaches this callback.
  final ValueChanged<RankedSticker> onSelect;

  /// Replaces the whole selection while a drag is in progress.
  final ValueChanged<Set<String>> onDragSelection;

  /// Secondary tap on a blank grid area during selection mode.
  final VoidCallback onExitSelection;
  final ValueChanged<RankedSticker> onCopy;
  final ValueChanged<RankedSticker>? onLongPress;
  final ValueChanged<RankedSticker> onPin;
  final ValueChanged<RankedSticker> onGroups;
  final ValueChanged<RankedSticker> onEdit;
  final ValueChanged<RankedSticker> onDelete;
  final Future<void> Function(RankedSticker entry, Offset globalPosition)
      onContextMenu;

  @override
  State<StickerGrid> createState() => _StickerGridState();
}

class _StickerGridState extends State<StickerGrid> {
  GridMetrics? _metrics;
  ui.Offset? _dragStart;
  ui.Rect? _dragRect;
  Set<String>? _dragBase;
  bool _dragAppend = false;
  bool _dragActive = false;
  bool _pointerSecondary = false;
  DateTime? _suppressTapUntil;
  String? _dragTapStickerId;

  bool get _draggingEnabled => widget.selectionMode && isDesktopPlatform;

  @override
  void didUpdateWidget(covariant StickerGrid oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.selectionMode && !widget.selectionMode) {
      _clearDrag();
      _suppressTapUntil = null;
      _dragTapStickerId = null;
    }
  }

  void _clearDrag() {
    _dragStart = null;
    _dragRect = null;
    _dragBase = null;
    _dragAppend = false;
    _dragActive = false;
    _pointerSecondary = false;
  }

  bool get _isControlPressed {
    final pressed = HardwareKeyboard.instance.logicalKeysPressed;
    return pressed.contains(LogicalKeyboardKey.controlLeft) ||
        pressed.contains(LogicalKeyboardKey.controlRight) ||
        pressed.contains(LogicalKeyboardKey.metaLeft) ||
        pressed.contains(LogicalKeyboardKey.metaRight);
  }

  double get _scrollOffset =>
      widget.scrollController.hasClients ? widget.scrollController.offset : 0.0;

  String? _stickerIdAtPoint(ui.Offset point) {
    final metrics = _metrics;
    if (metrics == null || widget.stickers.isEmpty) return null;
    for (var index = 0; index < widget.stickers.length; index++) {
      if (metrics
          .tileRect(index, scrollOffset: _scrollOffset)
          .contains(point)) {
        return widget.stickers[index].sticker.id;
      }
    }
    return null;
  }

  Set<String> _stickerIdsInRect(ui.Rect rect) {
    final metrics = _metrics;
    final selected = <String>{};
    if (metrics == null) return selected;
    for (var index = 0; index < widget.stickers.length; index++) {
      if (rect.overlaps(metrics.tileRect(index, scrollOffset: _scrollOffset))) {
        selected.add(widget.stickers[index].sticker.id);
      }
    }
    return selected;
  }

  void _handlePointerDown(PointerDownEvent event) {
    if (!_draggingEnabled) return;
    final isSecondary = (event.buttons & kSecondaryMouseButton) != 0;
    final isPrimary = (event.buttons & kPrimaryMouseButton) != 0;
    if (isSecondary) {
      _pointerSecondary = true;
      // A card owns its secondary tap and opens its management menu. Only a
      // blank grid area cancels the current selection.
      if (_stickerIdAtPoint(event.localPosition) == null) {
        widget.onExitSelection();
      }
      return;
    }
    if (!isPrimary) return;
    _pointerSecondary = false;
    final append = _isControlPressed;
    _dragStart = event.localPosition;
    _dragAppend = append;
    _dragBase = append ? Set<String>.of(widget.selectedIds) : <String>{};
    _dragTapStickerId = _stickerIdAtPoint(event.localPosition);
    _suppressTapUntil = null;
    _dragRect = ui.Rect.fromPoints(event.localPosition, event.localPosition);
    _dragActive = false;
    setState(() {});
  }

  void _handlePointerMove(PointerMoveEvent event) {
    if (!_draggingEnabled ||
        _pointerSecondary ||
        (event.buttons & kPrimaryMouseButton) == 0) {
      return;
    }
    final start = _dragStart;
    final base = _dragBase;
    if (start == null || base == null) return;
    final current = event.localPosition;
    if (!_dragActive && (current - start).distance < 6) return;
    _dragActive = true;
    // The pointer-up following an active drag lands on the card under the
    // cursor and would otherwise read as a deliberate toggle click.
    if (_dragTapStickerId != null) {
      _suppressTapUntil = DateTime.now().add(const Duration(milliseconds: 500));
    }
    final rect = ui.Rect.fromPoints(start, current);
    final hits = _stickerIdsInRect(rect);
    setState(() => _dragRect = rect);
    widget.onDragSelection(_dragAppend ? (<String>{...base, ...hits}) : hits);
  }

  void _finishDrag() {
    if (_dragStart == null) return;
    final wasActive = _dragActive;
    setState(_clearDrag);
    if (!wasActive) {
      _suppressTapUntil = null;
      _dragTapStickerId = null;
    }
  }

  void _handlePointerUp(PointerUpEvent event) {
    if (!_draggingEnabled) return;
    if (_pointerSecondary) {
      _pointerSecondary = false;
      return;
    }
    _finishDrag();
  }

  void _handlePointerCancel(PointerCancelEvent event) {
    if (!_draggingEnabled) return;
    _pointerSecondary = false;
    _finishDrag();
  }

  void _handlePointerSignal(PointerSignalEvent event) {
    if (!_draggingEnabled) return;
    if (event is! PointerScrollEvent || !widget.scrollController.hasClients) {
      return;
    }
    final position = widget.scrollController.position;
    final target = (position.pixels + event.scrollDelta.dy)
        .clamp(0.0, position.maxScrollExtent)
        .toDouble();
    if ((target - position.pixels).abs() < 0.5) return;
    widget.scrollController.jumpTo(target);
  }

  void _handleCardSelect(RankedSticker entry) {
    final suppressUntil = _suppressTapUntil;
    if (suppressUntil != null) {
      final suppressed = DateTime.now().isBefore(suppressUntil) &&
          entry.sticker.id == _dragTapStickerId;
      // Any subsequent intentional card click clears the drag guard. This
      // prevents a previous drag from suppressing an unrelated selection.
      _suppressTapUntil = null;
      _dragTapStickerId = null;
      if (suppressed) return;
    }
    widget.onSelect(entry);
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final metrics = GridMetrics.resolve(
          availableWidth: constraints.maxWidth,
          density: widget.density,
          quickPicker: widget.quickPicker,
        );
        if (_metrics != metrics) {
          _metrics = metrics;
          // The page keeps a copy of the metrics for keyboard navigation.
          // Report after the build phase so it can setState safely.
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted && _metrics == metrics) {
              widget.onMetricsChanged(metrics);
            }
          });
        }
        return Stack(
          children: [
            Listener(
              behavior: HitTestBehavior.opaque,
              onPointerDown: widget.selectionMode ? _handlePointerDown : null,
              onPointerMove: widget.selectionMode ? _handlePointerMove : null,
              onPointerUp: widget.selectionMode ? _handlePointerUp : null,
              onPointerCancel:
                  widget.selectionMode ? _handlePointerCancel : null,
              onPointerSignal:
                  widget.selectionMode ? _handlePointerSignal : null,
              child: GridView.builder(
                controller: widget.scrollController,
                physics: isDesktopPlatform && widget.selectionMode
                    ? const NeverScrollableScrollPhysics()
                    : null,
                padding: EdgeInsets.all(metrics.padding),
                gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                  crossAxisCount: metrics.columnCount,
                  mainAxisExtent: metrics.tileHeight,
                  crossAxisSpacing: metrics.spacing,
                  mainAxisSpacing: metrics.spacing,
                ),
                itemCount: widget.stickers.length,
                itemBuilder: (context, index) {
                  final entry = widget.stickers[index];
                  return StickerCard(
                    key: ValueKey<String>(entry.sticker.id),
                    entry: entry,
                    selectionMode: widget.selectionMode,
                    selected: widget.selectedIds.contains(entry.sticker.id),
                    keyboardFocused: widget.showKeyboardFocus &&
                        index == widget.focusedIndex,
                    onUse: () => widget.onUse(entry),
                    onSelect: () => _handleCardSelect(entry),
                    onCopy: () => widget.onCopy(entry),
                    // Quick mode is intentionally a send-only surface.
                    // Long-press is reserved for GIF preview on Android;
                    // Windows management mode keeps it as the multi-select
                    // entry.
                    onLongPress: widget.onLongPress == null
                        ? null
                        : () => widget.onLongPress!(entry),
                    onPin: () => widget.onPin(entry),
                    onGroups: () => widget.onGroups(entry),
                    onEdit: () => widget.onEdit(entry),
                    onDelete: () => widget.onDelete(entry),
                    onContextMenu: (position) =>
                        widget.onContextMenu(entry, position),
                    compact: widget.quickPicker,
                  );
                },
              ),
            ),
            if (_dragRect != null)
              Positioned.fill(
                child: IgnorePointer(
                  child: CustomPaint(
                    painter: _SelectionRectPainter(_dragRect!),
                  ),
                ),
              ),
          ],
        );
      },
    );
  }
}

class _SelectionRectPainter extends CustomPainter {
  const _SelectionRectPainter(this.rect);

  final ui.Rect rect;

  @override
  void paint(ui.Canvas canvas, ui.Size size) {
    final fill = ui.Paint()
      ..color = const Color(0x26087F78)
      ..style = ui.PaintingStyle.fill;
    final border = ui.Paint()
      ..color = AppTheme.accent
      ..style = ui.PaintingStyle.stroke
      ..strokeWidth = 1.5;
    canvas.drawRect(rect, fill);
    canvas.drawRect(rect, border);
  }

  @override
  bool shouldRepaint(_SelectionRectPainter oldDelegate) =>
      oldDelegate.rect != rect;
}
