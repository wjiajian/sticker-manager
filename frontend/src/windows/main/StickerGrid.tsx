import { ArrowsDownUpIcon } from "@phosphor-icons/react";
import { useIsMutating } from "@tanstack/react-query";
import { Button } from "../../components/Button";
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import type { Sticker, SyncStatus } from "../../lib/ipc";
import { type useStickers, useCategories, useLibraryOverview } from "../../lib/queries";
import { GridSkeleton, useDelayedLoading } from "../../components/Skeleton";
import { ReadError } from "../../components/ReadError";
import { EmptyLibrary } from "./EmptyLibrary";
import { useUiStore, type StickerTypeFilter } from "../../stores/ui";
import { LIBRARY_VIEWS } from "./libraryViews";
import { RecentStickers } from "./RecentStickers";
import type { Usage } from "./stickerList";
import { StickerTile, type SelectMode } from "./StickerTile";
import { TileSizeSlider } from "./TileSizeSlider";
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
  const selectedIds = useUiStore((s) => s.selectedStickerIds);
  const selectSticker = useUiStore((s) => s.selectSticker);
  const setSelection = useUiStore((s) => s.setSelection);
  const resultFocusRequested = useUiStore((s) => s.resultFocusRequested);
  const requestResultFocus = useUiStore((s) => s.requestResultFocus);
  const firstTile = useRef<HTMLButtonElement | null>(null);
  const scroller = useRef<HTMLElement>(null);
  const grid = useRef<HTMLElement>(null);
  // 网格只挂载看得见的几行，第一张可能还没挂载：先滚到网格开头，挂载时再聚焦。
  const focusFirstWhenMounted = useRef(false);
  const firstTileRef = useCallback((button: HTMLButtonElement | null) => {
    firstTile.current = button;
    if (!button || !focusFirstWhenMounted.current) return;
    requestAnimationFrame(() => {
      if (!focusFirstWhenMounted.current || !button.isConnected) return;
      focusFirstWhenMounted.current = false;
      button.focus();
      button.click();
    });
  }, []);
  // 继续输入会取消聚焦请求（setSearchText 清掉 resultFocusRequested），还在等第一张挂载的那次也作废，
  // 不然会在输入时选中上一次搜索的结果。订阅在 set 时同步触发，赶在第一张挂载之前。
  useEffect(() => useUiStore.subscribe((state, previous) => {
    if (state.searchText !== previous.searchText) focusFirstWhenMounted.current = false;
  }), []);
  const mutating = useIsMutating() > 0;
  const { data: source, error, isPending } = listing;
  const skeleton = useDelayedLoading(isPending, searchQuery || (typeof view === "string" ? view : view.category));
  const overview = useLibraryOverview();
  const menus = useStickerMenu(actions);
  const sortMenu = useSortMenu();
  const [tileSizeError, setTileSizeError] = useState<string | null>(null);
  const recentlyDeleted = !searchQuery && view === "recentlyDeleted";
  // 单个分类里每张都一样；最近删除的格子底部写着剩余天数。
  const showCategory = Boolean(searchQuery) || (typeof view === "string" && view !== "recentlyDeleted");
  const { data: categories } = useCategories();
  const label = searchQuery ? `搜索“${searchQuery}”` : typeof view === "string"
    ? LIBRARY_VIEWS.find((v) => v.view === view)?.label
    : categories?.find((category) => category.id === view.category)?.name;
  // 换搜索词后，新结果回来之前网格里还是上一次的结果，这时不能据此取消选中或选中第一张。
  const results = searchQuery && source && !listing.isPlaceholderData ? stickers : undefined;
  const filteredEmpty = Boolean(source?.length) && stickers.length === 0 && typeFilter !== "all";

  useEffect(() => {
    if (!selectedId || typeFilter === "all") return;
    const selected = source?.find((sticker) => sticker.id === selectedId);
    if (selected && selected.animated !== (typeFilter === "animated")) selectSticker(null);
  }, [source, typeFilter, selectedId, selectSticker]);

  // 多选里的表情不在列表里了（被其他设备删除、筛选掉等），就从多选里去掉。
  useEffect(() => {
    if (selectedIds.length < 2 || isPending || listing.isPlaceholderData) return;
    const present = selectedIds.filter((id) => stickers.some((sticker) => sticker.id === id));
    if (present.length === selectedIds.length) return;
    if (present.length > 1) setSelection(present, present.includes(selectedId ?? "") ? selectedId : present[0]);
    else selectSticker(present[0] ?? null);
  }, [stickers, selectedIds, selectedId, isPending, listing.isPlaceholderData, selectSticker, setSelection]);

  function choose(sticker: Sticker, mode: SelectMode) {
    const anchor = stickers.findIndex((item) => item.id === selectedId);
    if (mode === "range" && anchor >= 0) {
      const to = stickers.findIndex((item) => item.id === sticker.id);
      const [from, end] = anchor < to ? [anchor, to] : [to, anchor];
      const ids = stickers.slice(from, end + 1).map((item) => item.id);
      setSelection(ids, selectedId);
    } else if (mode === "toggle") {
      const ids = selectedIds.includes(sticker.id) ? selectedIds.filter((id) => id !== sticker.id) : [...selectedIds, sticker.id];
      if (ids.length < 2) selectSticker(ids[0] ?? null);
      else setSelection(ids, selectedIds.includes(sticker.id) ? ids[ids.length - 1] : sticker.id);
    } else {
      selectSticker(sticker.id);
    }
  }

  useEffect(() => {
    if (!results) return;
    if (selectedId && !results.some((sticker) => sticker.id === selectedId)) selectSticker(null);
    if (resultFocusRequested) {
      requestResultFocus(false);
      if (firstTile.current) {
        firstTile.current.focus();
        firstTile.current.click();
      } else if (results.length) {
        focusFirstWhenMounted.current = true;
        grid.current?.scrollIntoView({ block: "start" });
      }
    }
  }, [results, selectedId, selectSticker, resultFocusRequested, requestResultFocus]);

  return (
    <main ref={scroller} className="flex min-w-0 flex-1 flex-col gap-6 overflow-y-auto px-8 py-6" aria-busy={isPending}>
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex min-w-0 max-w-full flex-col gap-1">
          <div className="flex items-baseline gap-3">
            <h1 id="sticker-grid-title" tabIndex={-1} title={label} className="min-w-0 truncate text-2xl font-semibold">{label}</h1>
            {source && <span className="shrink-0 text-ink-secondary">{stickers.length} 张</span>}
          </div>
          {recentlyDeleted && <p className="text-xs text-ink-secondary">表情会在删除 30 天后彻底删除</p>}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {/* 清空整个最近删除，不受类型筛选影响；确认框写明张数。 */}
          {recentlyDeleted && !!source?.length && <Button
            disabled={busy || actions.busy || mutating || listing.isFetching}
            className="disabled:pointer-events-none disabled:opacity-50"
            onClick={() => void actions.purgeDeletedStickers(source.map((sticker) => sticker.id))}>
            清空最近删除…
          </Button>}
          <div role="group" aria-label="类型" className="flex gap-0.5 rounded-control bg-muted p-0.5">
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
          <TileSizeSlider onError={setTileSizeError} />
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
            {searchQuery ? <p className="text-ink-secondary">可以搜表情名、分类和配文</p> : (view === "all" || view === "uncategorized") && (
              <p className="text-ink-secondary">点击“导入表情”，或把图片和文件夹拖入窗口</p>
            )}
          </>}
        </div>
      ) : (
        <TileGrid label={label} count={stickers.length} scroller={scroller} grid={grid} renderTile={(index) => {
          const sticker = stickers[index];
          return <StickerTile
            key={sticker.id}
            buttonRef={index === 0 ? firstTileRef : undefined}
            sticker={sticker}
            selected={selectedIds.includes(sticker.id)}
            onSelect={(mode) => choose(sticker, mode)}
            onMenu={(x, y) => menus.popup(sticker, x, y)}
            actions={actions}
            showCategory={showCategory}
          />;
        }} />
      )}
      {menus.error && <p role="alert" className="text-xs text-ink-secondary">{menus.error}</p>}
      {sortMenu.error && <p role="alert" className="text-xs text-ink-secondary">{sortMenu.error}</p>}
      {tileSizeError && <p role="alert" className="text-xs text-ink-secondary">{tileSizeError}</p>}
    </main>
  );
}

/** 格子间距，与 GridSkeleton 的 gap-3 相同。 */
const GAP = 12;
/** 看得见的行之外，上下各多挂载几行，滚动和 Tab 切换时不露白。 */
const OVERSCAN_ROWS = 3;

/**
 * 只挂载看得见的几行，几千张表情也只渲染一屏左右的格子。列数按 auto-fill 的规则由网格宽度和
 * 格子大小算出，排法和 GridSkeleton 一致；格子是正方形，行高等于格宽。大小单独订阅，
 * 拖动滑块时只重排看得见的格子。
 */
function TileGrid({ label, count, scroller, grid, renderTile }: {
  label?: string;
  count: number;
  scroller: RefObject<HTMLElement | null>;
  grid: RefObject<HTMLElement | null>;
  renderTile: (index: number) => ReactNode;
}) {
  const size = useUiStore((s) => s.stickerTileSize);
  const [layout, setLayout] = useState({ columns: 1, pitch: size + GAP, start: 0, end: 0 });
  const measure = useCallback(() => {
    const section = grid.current;
    const container = scroller.current;
    if (!section || !container) return;
    const box = section.getBoundingClientRect();
    const columns = Math.max(1, Math.floor((box.width + GAP) / (size + GAP)));
    const pitch = (box.width - GAP * (columns - 1)) / columns + GAP;
    const rows = Math.ceil(count / columns);
    // 网格顶边相对滚动区可见部分顶边的位置，滚过之后是负数。
    const top = box.top - container.getBoundingClientRect().top;
    const start = Math.min(rows, Math.max(0, Math.floor(-top / pitch) - OVERSCAN_ROWS));
    const end = Math.min(rows, Math.max(start, Math.ceil((container.clientHeight - top) / pitch) + OVERSCAN_ROWS));
    setLayout((old) => old.columns === columns && old.pitch === pitch && old.start === start && old.end === end ? old : { columns, pitch, start, end });
  }, [grid, scroller, size, count]);

  // 大小、数量或上方内容变化后，在绘制前重算，不闪出空白或旧排法。
  useLayoutEffect(measure);
  useEffect(() => {
    const container = scroller.current;
    const section = grid.current;
    if (!container || !section) return;
    container.addEventListener("scroll", measure, { passive: true });
    // 窗口宽度变化，以及上方的最近使用、导入结果等改变高度时，网格的位置和列数都会变。
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    const observeChildren = () => { for (const child of container.children) observer.observe(child); };
    observeChildren();
    // 最近使用等区块可能晚于网格出现，出现后也要跟踪它们的高度。
    const children = new MutationObserver(() => { observeChildren(); measure(); });
    children.observe(container, { childList: true });
    return () => { container.removeEventListener("scroll", measure); observer.disconnect(); children.disconnect(); };
  }, [scroller, grid, measure]);

  const { columns, pitch, start, end } = layout;
  const rows = Math.ceil(count / columns);
  const first = start * columns;
  const last = Math.min(count, end * columns);
  return <section ref={grid} aria-label={label} className="shrink-0" style={{ height: rows ? rows * pitch - GAP : 0, paddingTop: start * pitch }}>
    <div className="grid" style={{ gap: GAP, gridTemplateColumns: `repeat(${columns},minmax(0,1fr))` }}>
      {Array.from({ length: Math.max(0, last - first) }, (_, offset) => renderTile(first + offset))}
    </div>
  </section>;
}
