import { useRef } from "react";
import type { Sticker } from "../../lib/ipc";
import type { StickerSort, StickerTypeFilter } from "../../stores/ui";

const nameCollator = new Intl.Collator("zh-CN", { numeric: true });

export type Usage = Pick<Sticker, "usageCount" | "lastUsedAt">;
type UsageOf = (sticker: Sticker) => Usage;
const actualUsage: UsageOf = (sticker) => sticker;

export function filterStickers(stickers: Sticker[], filter: StickerTypeFilter) {
  return filter === "all" ? stickers : stickers.filter((sticker) => sticker.animated === (filter === "animated"));
}

function byLastUsed(a: Usage, b: Usage) {
  if (a.lastUsedAt === b.lastUsedAt) return 0;
  if (a.lastUsedAt === null) return 1;
  if (b.lastUsedAt === null) return -1;
  return b.lastUsedAt - a.lastUsedAt;
}

export function sortStickers(stickers: Sticker[], sort: StickerSort, usage: UsageOf = actualUsage) {
  // 可排序视图的原始列表已按添加时间倒序；稳定排序保留同值项的这个顺序。
  // 先复制，避免改变查询缓存和其他视图使用的列表。
  switch (sort) {
    case "added": return stickers;
    case "recent": return [...stickers].sort((a, b) => byLastUsed(usage(a), usage(b)));
    case "usage": return [...stickers].sort((a, b) => usage(b).usageCount - usage(a).usageCount || byLastUsed(usage(a), usage(b)));
    case "name": return [...stickers].sort((a, b) => nameCollator.compare(a.name, b.name));
  }
}

/**
 * 按使用排序时，同一次打开里复制不让格子跳位：每张表情第一次出现时记下使用数据，
 * 之后都按这份记录排序；session 变了（重新打开、切换视图等）才按实际数据重排。
 * 显示的使用次数仍取实际数据。
 */
export function useUsageSnapshot(session: unknown): UsageOf {
  const snapshot = useRef<{ session: unknown; usage: UsageOf } | null>(null);
  if (!snapshot.current || snapshot.current.session !== session) {
    const seen = new Map<string, Usage>();
    snapshot.current = {
      session,
      usage: (sticker) => {
        let usage = seen.get(sticker.id);
        if (!usage) {
          usage = { usageCount: sticker.usageCount, lastUsedAt: sticker.lastUsedAt };
          seen.set(sticker.id, usage);
        }
        return usage;
      },
    };
  }
  return snapshot.current.usage;
}
