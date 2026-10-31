import { useEffect, useRef, type Ref } from "react";
import type { Sticker } from "../../lib/ipc";
import { useStickerSearch, useStickers, useTagSidebar } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";
import { LIBRARY_VIEWS } from "./libraryViews";
import { StickerImage } from "./StickerImage";
import { useDragOut } from "./useDragOut";
import type { StickerActions } from "./useStickerActions";

export function StickerGrid({ actions }: { actions: StickerActions }) {
  const view = useUiStore((s) => s.view);
  const searchQuery = useUiStore((s) => s.searchQuery);
  const selectedId = useUiStore((s) => s.selectedStickerId);
  const selectSticker = useUiStore((s) => s.selectSticker);
  const resultFocusRequested = useUiStore((s) => s.resultFocusRequested);
  const requestResultFocus = useUiStore((s) => s.requestResultFocus);
  const firstTile = useRef<HTMLButtonElement>(null);
  const listing = useStickers(view);
  const search = useStickerSearch(searchQuery);
  const { data: stickers, error, isPending } = searchQuery ? search : listing;
  const { data: sidebar } = useTagSidebar();
  const label = searchQuery ? `搜索“${searchQuery}”` : typeof view === "string"
    ? LIBRARY_VIEWS.find((v) => v.view === view)?.label
    : [...(sidebar?.groups.flatMap((group) => group.tags) ?? []), ...(sidebar?.ungrouped ?? [])].find((tag) => tag.id === view.tag)?.name;
  // 换搜索词后，新结果回来之前网格里还是上一次的结果，这时不能据此取消选中或选中第一张。
  const results = searchQuery && !search.isPlaceholderData ? search.data : undefined;

  useEffect(() => {
    if (!results) return;
    if (selectedId && !results.some((sticker) => sticker.id === selectedId)) selectSticker(null);
    if (resultFocusRequested) {
      requestResultFocus(false);
      firstTile.current?.focus();
      firstTile.current?.click();
    }
  }, [results, selectedId, selectSticker, resultFocusRequested, requestResultFocus]);

  return (
    <main className="flex min-w-0 flex-1 flex-col gap-6 overflow-y-auto px-8 py-6" aria-busy={isPending}>
      <div className="flex items-baseline gap-3">
        <h1 title={label} className="min-w-0 truncate text-2xl font-semibold">{label}</h1>
        {stickers && <span className="shrink-0 text-ink-secondary">{stickers.length} 张</span>}
      </div>

      {error ? (
        <p role="alert" className="text-ink-secondary">读取仓库失败：{String(error)}</p>
      ) : isPending ? (
        <p role="status" className="text-ink-secondary">正在读取表情…</p>
      ) : stickers.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-1 pb-16 text-center">
          <p className="text-base font-semibold">{searchQuery ? "没有找到相关表情" : "这里还没有表情"}</p>
          {searchQuery ? <p className="text-ink-secondary">可以搜表情名、标签和配文</p> : (view === "all" || view === "untagged") && (
            <p className="text-ink-secondary">点击“导入表情”，或把图片和文件夹拖入窗口</p>
          )}
        </div>
      ) : (
        <section aria-label={label} className="grid grid-cols-[repeat(auto-fill,minmax(104px,1fr))] gap-3">
          {stickers.map((sticker, index) => (
            <StickerTile
              key={sticker.id}
              buttonRef={index === 0 ? firstTile : undefined}
              sticker={sticker}
              selected={selectedId === sticker.id}
              onSelect={() => selectSticker(sticker.id)}
              actions={actions}
            />
          ))}
        </section>
      )}
    </main>
  );
}

function StickerTile({ sticker, selected, onSelect, actions, buttonRef }: {
  sticker: Sticker;
  selected: boolean;
  onSelect: () => void;
  actions: StickerActions;
  buttonRef?: Ref<HTMLButtonElement>;
}) {
  const drag = useDragOut((isPressed) => actions.drag(sticker, isPressed), sticker.deletedAt !== null || actions.busy);
  return (
    <button
      ref={buttonRef}
      type="button"
      aria-label={sticker.name}
      aria-pressed={selected}
      onClick={onSelect}
      {...drag}
      className={`interactive relative flex aspect-square min-h-0 w-full touch-none items-center justify-center rounded-card bg-tile p-4 hover:-translate-y-0.5 active:scale-98 ${
        selected ? "ring-2 ring-accent ring-offset-2 ring-offset-surface" : ""
      }`}
    >
      <StickerImage path={sticker.thumbnailPath} name={sticker.name} lazy />
      {sticker.caption && <span title={sticker.caption} className="absolute bottom-2 left-1/2 max-w-[calc(100%-1rem)] -translate-x-1/2 truncate rounded-sm bg-ink px-2 py-0.5 text-xs font-semibold text-surface windows:rounded-xs">{sticker.caption}</span>}
      {sticker.animated && (
        <span aria-label="动图" className="absolute top-2 right-2 rounded-sm bg-surface px-1 text-xs font-semibold windows:rounded-xs">GIF</span>
      )}
    </button>
  );
}
