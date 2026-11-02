import { ArrowsDownUpIcon } from "@phosphor-icons/react";
import { useEffect, useRef } from "react";
import type { Sticker, SyncStatus } from "../../lib/ipc";
import { type useStickers, useTagSidebar, useLibraryOverview } from "../../lib/queries";
import { GridSkeleton, useDelayedLoading } from "../../components/Skeleton";
import { ReadError } from "../../components/ReadError";
import { EmptyLibrary } from "./EmptyLibrary";
import { useUiStore, type StickerTypeFilter } from "../../stores/ui";
import { LIBRARY_VIEWS } from "./libraryViews";
import { RecentStickers } from "./RecentStickers";
import type { Usage } from "./stickerList";
import { StickerTile } from "./StickerTile";
import { STICKER_SORTS, useSortMenu } from "./useSortMenu";
import type { StickerActions } from "./useStickerActions";
import { useStickerMenu } from "./useStickerMenu";

const TYPE_FILTERS: { value: StickerTypeFilter; label: string }[] = [
  { value: "all", label: "全部" },
  { value: "static", label: "静态" },
  { value: "animated", label: "动图" },
];

export function StickerGrid({ actions, listing, stickers, sortable, usage, sync, busy, onChoose, onSyncSettings, shortcutAvailable }: {
  actions: StickerActions;
  listing: ReturnType<typeof useStickers>;
  stickers: Sticker[];
  sortable: boolean;
  usage: (sticker: Sticker) => Usage;
  sync?: SyncStatus; busy: boolean; onChoose: (directory: boolean) => void; onSyncSettings: () => void; shortcutAvailable: boolean;
}) {
  const view = useUiStore((s) => s.view);
  const searchQuery = useUiStore((s) => s.searchQuery);
  const typeFilter = useUiStore((s) => s.typeFilter);
  const setTypeFilter = useUiStore((s) => s.setTypeFilter);
  const sortOrder = useUiStore((s) => s.sortOrder);
  const selectedId = useUiStore((s) => s.selectedStickerId);
  const selectSticker = useUiStore((s) => s.selectSticker);
  const resultFocusRequested = useUiStore((s) => s.resultFocusRequested);
  const requestResultFocus = useUiStore((s) => s.requestResultFocus);
  const firstTile = useRef<HTMLButtonElement>(null);
  const { data: source, error, isPending } = listing;
  const skeleton = useDelayedLoading(isPending, searchQuery || (typeof view === "string" ? view : view.tag));
  const overview = useLibraryOverview();
  const menus = useStickerMenu(actions);
  const sortMenu = useSortMenu();
  const recentlyDeleted = !searchQuery && view === "recentlyDeleted";
  const { data: sidebar } = useTagSidebar();
  const label = searchQuery ? `搜索“${searchQuery}”` : typeof view === "string"
    ? LIBRARY_VIEWS.find((v) => v.view === view)?.label
    : [...(sidebar?.groups.flatMap((group) => group.tags) ?? []), ...(sidebar?.ungrouped ?? [])].find((tag) => tag.id === view.tag)?.name;
  // 换搜索词后，新结果回来之前网格里还是上一次的结果，这时不能据此取消选中或选中第一张。
  const results = searchQuery && source && !listing.isPlaceholderData ? stickers : undefined;
  const filteredEmpty = Boolean(source?.length) && stickers.length === 0 && typeFilter !== "all";

  useEffect(() => {
    if (!selectedId || typeFilter === "all") return;
    const selected = source?.find((sticker) => sticker.id === selectedId);
    if (selected && selected.animated !== (typeFilter === "animated")) selectSticker(null);
  }, [source, typeFilter, selectedId, selectSticker]);

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
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex min-w-0 max-w-full flex-col gap-1">
          <div className="flex items-baseline gap-3">
            <h1 title={label} className="min-w-0 truncate text-2xl font-semibold">{label}</h1>
            {source && <span className="shrink-0 text-ink-secondary">{stickers.length} 张</span>}
          </div>
          {recentlyDeleted && <p className="text-xs text-ink-secondary">表情会在删除 30 天后彻底删除</p>}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <div role="group" aria-label="类型" className="flex gap-0.5 rounded-control bg-tile p-0.5">
            {TYPE_FILTERS.map(({ value, label: filterLabel }) => (
              <button
                key={value}
                type="button"
                aria-pressed={typeFilter === value}
                onClick={() => setTypeFilter(value)}
                className={`interactive rounded-[calc(var(--radius-control)-2px)] px-3 py-1 hover:opacity-82 active:scale-98 ${
                  typeFilter === value ? "bg-surface font-semibold text-ink" : "bg-transparent text-ink-tertiary"
                }`}
              >
                {filterLabel}
              </button>
            ))}
          </div>
          {sortable && (
            <button
              type="button"
              aria-haspopup="menu"
              onClick={(event) => {
                const rect = event.currentTarget.getBoundingClientRect();
                void sortMenu.popup(rect.left, rect.bottom);
              }}
              className="interactive flex items-center gap-2 rounded-control border border-line bg-surface px-3 py-1 text-ink hover:opacity-82 active:scale-98"
            >
              <ArrowsDownUpIcon size={16} aria-hidden="true" />
              {STICKER_SORTS.find((sort) => sort.value === sortOrder)?.label}
            </button>
          )}
        </div>
      </div>

      {view === "all" && !searchQuery && <RecentStickers actions={actions} usage={usage} onMenu={menus.popup} />}

      {error ? (
        <ReadError error={error} icon busy={listing.isFetching} retry={() => { void listing.refetch(); }} />
      ) : isPending ? (
        skeleton ? <GridSkeleton /> : <div className="flex-1" />
      ) : stickers.length === 0 ? (
        !searchQuery && typeFilter === "all" ? <EmptyLibrary view={view} label={label} all={overview.data?.all} sync={sync} busy={busy}
          onChoose={onChoose} onSyncSettings={onSyncSettings} shortcutAvailable={shortcutAvailable} />
        : <div className="flex flex-1 flex-col items-center justify-center gap-1 pb-16 text-center">
          {filteredEmpty ? <>
            <p className="text-base font-semibold">{typeFilter === "animated" ? "没有动图" : "没有静态表情"}</p>
            <p className="text-ink-secondary">切换到“全部”查看其他表情</p>
          </> : <>
            <p className="text-base font-semibold">{searchQuery ? "没有找到相关表情" : recentlyDeleted ? "最近删除里没有表情" : "这里还没有表情"}</p>
            {recentlyDeleted && <p className="text-ink-secondary">删除的表情会在这里保留 30 天</p>}
            {searchQuery ? <p className="text-ink-secondary">可以搜表情名、标签和配文</p> : (view === "all" || view === "untagged") && (
              <p className="text-ink-secondary">点击“导入表情”，或把图片和文件夹拖入窗口</p>
            )}
          </>}
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
              onMenu={(x, y) => menus.popup(sticker, x, y)}
              actions={actions}
            />
          ))}
        </section>
      )}
      {menus.error && <p role="alert" className="text-xs text-ink-secondary">{menus.error}</p>}
      {sortMenu.error && <p role="alert" className="text-xs text-ink-secondary">{sortMenu.error}</p>}
    </main>
  );
}
