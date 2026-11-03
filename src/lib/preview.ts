import type { PreviewSticker, Sticker, TagSidebar } from "./ipc";

const NO_ORDER = new Map<string, number>();
// 每份侧栏数据只建一次标签顺序表；面板每次渲染要给每个格子生成说明。
const orders = new WeakMap<TagSidebar, Map<string, number>>();

function tagOrder(sidebar?: TagSidebar) {
  if (!sidebar) return NO_ORDER;
  let order = orders.get(sidebar);
  if (!order) {
    order = new Map([...sidebar.groups.flatMap((group) => group.tags), ...sidebar.ungrouped].map((tag, index) => [tag.id, index]));
    orders.set(sidebar, order);
  }
  return order;
}

export function previewSticker(sticker: Sticker, sidebar?: TagSidebar): PreviewSticker {
  const order = tagOrder(sidebar);
  return {
    name: sticker.name,
    usageCount: sticker.usageCount,
    tags: [...sticker.tags].sort((a, b) => (order.get(a.id) ?? Infinity) - (order.get(b.id) ?? Infinity)).map((tag) => tag.name),
    thumbnailPath: sticker.thumbnailPath,
    objectPath: sticker.objectPath,
  };
}

export function previewDescription(sticker: PreviewSticker) {
  const usage = sticker.usageCount === 0 ? "还没用过" : `用过 ${sticker.usageCount} 次`;
  return sticker.tags.length ? `${usage} · ${sticker.tags.join(" ")}` : usage;
}
