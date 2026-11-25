import { dragDropClientPoint, type DragDropPosition } from "../../lib/dragDropPosition";
import { platform } from "../../lib/platform";
import type { StickerDropTarget } from "../../stores/stickerDrag";

function clientPoint(position: DragDropPosition) {
  return dragDropClientPoint(position, platform, window.devicePixelRatio);
}

export function stickerDropTargetAt(position: DragDropPosition): StickerDropTarget | null {
  const { x, y } = clientPoint(position);
  const row = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-sticker-drop]");
  const value = row?.dataset.stickerDrop;
  if (value === "uncategorized") return { kind: "uncategorized" };
  return value?.startsWith("category:") ? { kind: "category", id: value.slice("category:".length) } : null;
}

export function sidebarScrollDirection(rect: DOMRect, y: number) {
  if (y < rect.top + 24) return -1;
  if (y > rect.bottom - 24) return 1;
  return 0;
}

export function scrollStickerSidebar(position: DragDropPosition) {
  const { x, y } = clientPoint(position);
  const scroller = document.elementFromPoint(x, y)?.closest("nav div.overflow-y-auto");
  if (scroller) scroller.scrollBy(0, 16 * sidebarScrollDirection(scroller.getBoundingClientRect(), y));
}
