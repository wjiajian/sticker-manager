import { useLayoutEffect, useMemo, useRef, useState } from "react";
import type { Sticker } from "../../lib/ipc";
import { useStickers } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";
import { StickerTile } from "./StickerTile";
import { filterStickers } from "./stickerList";
import type { StickerActions } from "./useStickerActions";

export function RecentStickers({ actions, onMenu }: {
  actions: StickerActions;
  onMenu: (sticker: Sticker, x: number, y: number) => Promise<void>;
}) {
  const { data } = useStickers("recent");
  const typeFilter = useUiStore((s) => s.typeFilter);
  const selectedId = useUiStore((s) => s.selectedStickerId);
  const selectSticker = useUiStore((s) => s.selectSticker);
  const setView = useUiStore((s) => s.setView);
  const stickers = useMemo(() => filterStickers(data ?? [], typeFilter), [data, typeFilter]);
  const row = useRef<HTMLDivElement>(null);
  const [capacity, setCapacity] = useState(0);
  const hasStickers = stickers.length > 0;

  useLayoutEffect(() => {
    const element = row.current;
    if (!element) return;
    // 64px 小格、8px 间距；不渲染放不下的格子，Tab 和读屏也只访问可见项。
    const measure = (width: number) => setCapacity(Math.max(0, Math.min(10, Math.floor((width + 8) / 72))));
    measure(element.clientWidth);
    const observer = new ResizeObserver(([entry]) => measure(entry.contentRect.width));
    observer.observe(element);
    return () => observer.disconnect();
  }, [hasStickers]);

  if (!hasStickers) return null;

  return (
    <section aria-labelledby="recent-stickers-heading" className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <h2 id="recent-stickers-heading" className="text-sm font-semibold">最近使用</h2>
        <button type="button" onClick={() => setView("recent")} className="interactive rounded-control text-xs text-ink-tertiary hover:opacity-82 active:scale-98">
          查看全部 {stickers.length} 张
        </button>
      </div>
      <div ref={row} className="flex h-16 gap-2">
        {stickers.slice(0, capacity).map((sticker) => (
          <StickerTile
            key={sticker.id}
            sticker={sticker}
            compact
            selected={selectedId === sticker.id}
            onSelect={() => selectSticker(sticker.id)}
            onMenu={(x, y) => onMenu(sticker, x, y)}
            actions={actions}
          />
        ))}
      </div>
    </section>
  );
}
