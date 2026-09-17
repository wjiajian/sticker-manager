import 'dart:math' as math;
import 'dart:ui' as ui;

import '../models.dart';

/// Single source of truth for sticker grid geometry. Rendering, drag
/// selection hit testing, blank-area detection and keyboard scrolling all
/// derive their positions from one [GridMetrics] instance, so they can never
/// disagree about where a card sits.
class GridMetrics {
  const GridMetrics._({
    required this.columnCount,
    required this.padding,
    required this.spacing,
    required this.tileWidth,
    required this.tileHeight,
  });

  factory GridMetrics.resolve({
    required double availableWidth,
    GridDensity density = GridDensity.standard,
    bool quickPicker = false,
  }) {
    final padding = quickPicker
        ? 10.0
        : density == GridDensity.compact
            ? 20.0
            : 24.0;
    final spacing = quickPicker
        ? 8.0
        : density == GridDensity.compact
            ? 12.0
            : 16.0;
    final targetTileWidth = quickPicker
        ? 112.0
        : density == GridDensity.compact
            ? 148.0
            : 180.0;
    final tileHeight = quickPicker
        ? 132.0
        : density == GridDensity.compact
            ? 176.0
            : 204.0;
    final innerWidth = math.max(0.0, availableWidth - padding * 2);
    // Largest column count whose tiles still reach the target width, so
    // tiles grow to fill the row instead of leaving a ragged right edge.
    final columnCount = math.max(
      1,
      ((innerWidth + spacing) / (targetTileWidth + spacing)).floor(),
    );
    final tileWidth = (innerWidth - spacing * (columnCount - 1)) / columnCount;
    return GridMetrics._(
      columnCount: columnCount,
      padding: padding,
      spacing: spacing,
      tileWidth: tileWidth,
      tileHeight: tileHeight,
    );
  }

  final int columnCount;
  final double padding;
  final double spacing;
  final double tileWidth;
  final double tileHeight;

  double get rowExtent => tileHeight + spacing;

  /// Position of [index] inside the grid viewport. Pass the current scroll
  /// offset to convert content coordinates into viewport coordinates.
  ui.Rect tileRect(int index, {double scrollOffset = 0}) {
    final row = index ~/ columnCount;
    final column = index % columnCount;
    return ui.Rect.fromLTWH(
      padding + column * (tileWidth + spacing),
      padding + row * rowExtent - scrollOffset,
      tileWidth,
      tileHeight,
    );
  }

  @override
  bool operator ==(Object other) {
    return other is GridMetrics &&
        other.columnCount == columnCount &&
        other.padding == padding &&
        other.spacing == spacing &&
        other.tileWidth == tileWidth &&
        other.tileHeight == tileHeight;
  }

  @override
  int get hashCode =>
      Object.hash(columnCount, padding, spacing, tileWidth, tileHeight);
}
