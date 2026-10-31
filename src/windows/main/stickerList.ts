import type { Sticker } from "../../lib/ipc";
import type { StickerSort, StickerTypeFilter } from "../../stores/ui";

const nameCollator = new Intl.Collator("zh-CN", { numeric: true });

export function filterStickers(stickers: Sticker[], filter: StickerTypeFilter) {
  return filter === "all" ? stickers : stickers.filter((sticker) => sticker.animated === (filter === "animated"));
}

function byLastUsed(a: Sticker, b: Sticker) {
  if (a.lastUsedAt === b.lastUsedAt) return 0;
  if (a.lastUsedAt === null) return 1;
  if (b.lastUsedAt === null) return -1;
  return b.lastUsedAt - a.lastUsedAt;
}

export function sortStickers(stickers: Sticker[], sort: StickerSort) {
  // 可排序视图的原始列表已按添加时间倒序；稳定排序保留同值项的这个顺序。
  // 先复制，避免改变查询缓存和其他视图使用的列表。
  switch (sort) {
    case "added": return stickers;
    case "recent": return [...stickers].sort(byLastUsed);
    case "usage": return [...stickers].sort((a, b) => b.usageCount - a.usageCount || byLastUsed(a, b));
    case "name": return [...stickers].sort((a, b) => nameCollator.compare(a.name, b.name));
  }
}
