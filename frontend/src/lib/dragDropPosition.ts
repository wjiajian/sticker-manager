import type { Platform } from "./platform";

export type DragDropPosition = { x: number; y: number };

/** WebView2 提供物理像素，WKWebView 和 WebKitGTK 提供可直接用于视口命中检测的逻辑坐标。 */
export function dragDropClientPoint(
  position: DragDropPosition,
  platform: Platform,
  devicePixelRatio: number,
): DragDropPosition {
  if (platform !== "windows") return { x: position.x, y: position.y };
  const scale = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1;
  return { x: position.x / scale, y: position.y / scale };
}
