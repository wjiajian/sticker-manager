export type GridArrowKey = "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown";

export function isGridArrowKey(key: string): key is GridArrowKey {
  return key === "ArrowLeft" || key === "ArrowRight" || key === "ArrowUp" || key === "ArrowDown";
}

/** 左右按排列顺序跨行，上下按列移动。下一行缺这一列时，选中最后一张。 */
export function gridNavigationTarget(key: GridArrowKey, index: number, count: number, columns: number): number {
  if (!(count >= 1) || !Number.isInteger(index) || index < 0 || index >= count) return index;
  const perRow = Number.isFinite(columns) && columns >= 1 ? Math.floor(columns) : 1;
  switch (key) {
    case "ArrowLeft": return Math.max(0, index - 1);
    case "ArrowRight": return Math.min(count - 1, index + 1);
    case "ArrowUp": return index - perRow >= 0 ? index - perRow : index;
    case "ArrowDown": {
      if (index + perRow < count) return index + perRow;
      const lastRow = Math.floor((count - 1) / perRow);
      return Math.floor(index / perRow) < lastRow ? count - 1 : index;
    }
  }
}

/** 尽量少滚动，让格子连同上下留白完整可见。放不下时优先显示顶边。 */
export function revealScrollTop(scrollTop: number, viewportHeight: number, itemTop: number, itemBottom: number, margin: number): number {
  let next = scrollTop;
  if (itemBottom + margin > scrollTop + viewportHeight) next = itemBottom + margin - viewportHeight;
  if (itemTop - margin < next) next = itemTop - margin;
  return Math.max(0, next);
}
