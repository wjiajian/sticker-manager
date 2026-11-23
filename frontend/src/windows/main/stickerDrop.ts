import type { StickerDropTarget } from "../../stores/stickerDrag";

type Position = { x: number; y: number };

export function stickerDropTargetAt(position: Position): StickerDropTarget | null {
  const scale = window.devicePixelRatio || 1;
  const row = document.elementFromPoint(position.x / scale, position.y / scale)?.closest<HTMLElement>("[data-sticker-drop]");
  const value = row?.dataset.stickerDrop;
  if (value === "uncategorized") return { kind: "uncategorized" };
  return value?.startsWith("category:") ? { kind: "category", id: value.slice("category:".length) } : null;
}

export function sidebarScrollDirection(rect: DOMRect, y: number) {
  if (y < rect.top + 24) return -1;
  if (y > rect.bottom - 24) return 1;
  return 0;
}

export function scrollStickerSidebar(position: Position) {
  const scale = window.devicePixelRatio || 1;
  const x = position.x / scale;
  const y = position.y / scale;
  const scroller = document.elementFromPoint(x, y)?.closest("nav div.overflow-y-auto");
  if (scroller) scroller.scrollBy(0, 16 * sidebarScrollDirection(scroller.getBoundingClientRect(), y));
}
