import { MagnifyingGlassIcon, PushPinIcon } from "@phosphor-icons/react";
import { skipToken, useQuery, useQueryClient } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useSyncStatus, syncLabel, deviceTime, syncStateLabel, restoringLibrary } from "../main/useSyncStatus";
import { SyncIcon } from "../main/SyncIcon";
import { TagDot } from "../../components/TagDot";
import { ipc, type Sticker } from "../../lib/ipc";
import { useLibraryChangedEvents, useLibraryOverview, useStickers, useTagSidebar } from "../../lib/queries";
import { StickerImage } from "../main/StickerImage";
import { sortStickers, useUsageSnapshot } from "../main/stickerList";
import { useDragOut } from "../main/useDragOut";
import { usePanelActions } from "./usePanelActions";
import { previewDescription, previewSticker } from "../../lib/preview";
import { usePreview } from "./usePreview";
import { GridSkeleton, TagSkeleton, useDelayedLoading } from "../../components/Skeleton";

const EMPTY: Sticker[] = [];

export function Panel() {
  const sync = useSyncStatus();
  const syncTitle = sync.data?.phase === "synced" ? `${syncLabel(sync.data)} · ${deviceTime(sync.data.lastSyncAt)}` : syncStateLabel(sync.data);
  const client = useQueryClient();
  const input = useRef<HTMLInputElement>(null);
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  const [composing, setComposing] = useState(false);
  const composingRef = useRef(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tagId, setTagId] = useState<string | null>(null);
  const [pinned, setPinned] = useState(false);
  const [pinBusy, setPinBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // 每次打开面板换一份使用数据记录：打开期间复制不让最近使用和标签网格跳位。
  const [session, setSession] = useState(0);
  const usage = useUsageSnapshot(session);
  const overview = useLibraryOverview();
  const recentQuery = useStickers("recent");
  const sidebar = useTagSidebar();
  const eventError = useLibraryChangedEvents();
  const actions = usePanelActions();
  const resetActions = actions.reset;
  const tags = useMemo(() => sidebar.data
    ? [...sidebar.data.groups.flatMap((group) => group.tags), ...sidebar.data.ungrouped].filter((tag) => tag.stickerCount > 0)
    : [], [sidebar.data]);
  const currentTag = tags.find((tag) => tag.id === tagId) ?? tags[0];
  const tagged = useQuery({
    queryKey: ["library", "stickers", { tag: currentTag?.id }],
    queryFn: currentTag ? () => ipc.listStickers({ tag: currentTag.id }) : skipToken,
  });
  const tagStickers = useMemo(() => sortStickers(tagged.data ?? EMPTY, "recent", usage), [tagged.data, usage]);
  const search = useQuery({
    queryKey: ["library", "search", query],
    queryFn: query ? () => ipc.searchStickers(query) : skipToken,
  });
  const searching = text.length > 0;
  const searchReady = !composing && text.trim() === query && !search.isFetching && !search.isError;
  for (const list of [recentQuery.data, tagged.data, search.data]) list?.forEach(usage);
  const recent = useMemo(() => sortStickers(recentQuery.data ?? EMPTY, "recent", usage).slice(0, 8), [recentQuery.data, usage]);
  // 防抖期间不让 Enter 复制上一个词的结果。
  const choices = searching ? (searchReady ? search.data ?? EMPTY : EMPTY) : recent;
  const selected = choices.find((sticker) => sticker.id === selectedId) ?? choices[0];
  const choicePrefix = searching ? "search" : "recent";
  const activeId = selected ? `${choicePrefix}-${selected.id}` : undefined;
  const empty = overview.data?.all === 0;
  const recentSkeleton = useDelayedLoading(recentQuery.isPending || overview.isPending);
  const tagsSkeleton = useDelayedLoading(sidebar.isPending);
  const taggedSkeleton = useDelayedLoading(tagged.isPending && !!currentTag, currentTag?.id);
  const previews = new Map((empty ? [] : searching ? choices.map((sticker) => ["search", sticker] as const)
    : [...recent.map((sticker) => ["recent", sticker] as const), ...tagStickers.map((sticker) => ["tag", sticker] as const)])
    .map(([prefix, sticker]) => [`${prefix}-${sticker.id}`, previewSticker(sticker, sidebar.data)]));
  const preview = usePreview(previews, actions.busy, text, setError);

  useEffect(() => {
    if (currentTag) setTagId(currentTag.id);
  }, [currentTag]);

  useEffect(() => {
    if (composing) return;
    if (!text.trim()) { setQuery(""); return; }
    const timer = setTimeout(() => setQuery(text.trim()), 150);
    return () => clearTimeout(timer);
  }, [text, composing]);

  useEffect(() => {
    document.getElementById(activeId ?? "")?.scrollIntoView({ block: "nearest" });
  }, [activeId]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    function focus() { input.current?.focus({ preventScroll: true }); }
    function opened() {
      if (disposed) return;
      setText(""); setQuery(""); setSelectedId(null); setError(null); setSession((count) => count + 1);
      composingRef.current = false; setComposing(false);
      resetActions();
      void client.invalidateQueries({ queryKey: ["library"] });
      focus();
      requestAnimationFrame(() => { if (!disposed) focus(); });
    }
    const listeners = [listen("panel-shown", opened), listen("panel-focus", focus)];
    // 先装好监听再读取状态，覆盖窗口刚加载就按快捷键的情况。
    void Promise.all(listeners).then(() => ipc.panelSession()).then((session) => {
      if (disposed) return;
      setPinned(session.pinned);
      if (session.visible) opened();
    }).catch((reason: unknown) => { if (!disposed) setError(String(reason)); });
    return () => {
      disposed = true;
      for (const pending of listeners) void pending.then((unlisten) => unlisten(), () => {});
    };
  }, [client, resetActions]);

  async function run(operation: () => Promise<unknown>) {
    try { await operation(); }
    catch (reason) { setError(String(reason)); }
  }

  function clearSearch() { setText(""); setQuery(""); setSelectedId(null); }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (composingRef.current || event.nativeEvent.isComposing || event.keyCode === 229) return;
    if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
    if (event.key === "Escape") return; // 外层也支持按钮有焦点时按 Esc。
    if (!searching && /^[1-8]$/.test(event.key)) {
      event.preventDefault();
      const sticker = recent[Number(event.key) - 1];
      if (sticker && !event.repeat) void actions.copy(sticker, `recent-${sticker.id}`);
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (selected && !event.repeat) void actions.copy(selected, `${choicePrefix}-${selected.id}`);
    } else {
      const delta = event.key === "ArrowDown" ? 4 : event.key === "ArrowUp" ? -4
        : !searching && event.key === "ArrowRight" ? 1 : !searching && event.key === "ArrowLeft" ? -1 : 0;
      if (!delta) return;
      event.preventDefault();
      if (!selected) return;
      const index = choices.findIndex((sticker) => sticker.id === selected.id);
      setSelectedId(choices[Math.max(0, Math.min(choices.length - 1, index + delta))].id);
    }
  }

  function tile(sticker: Sticker, prefix: string, index?: number) {
    const key = `${prefix}-${sticker.id}`;
    return <PanelTile key={key} id={key} sticker={sticker} number={index === undefined ? undefined : index + 1}
      selectable={prefix !== "tag"} selected={activeId === key} copied={actions.copiedKey === key}
      busy={actions.busy} onCopy={() => { void actions.copy(sticker, key); }}
      preview={preview.tile(key)} description={previewDescription(previewSticker(sticker, sidebar.data))}
      onDrag={(isPressed) => actions.drag(sticker, isPressed)} />;
  }

  const problem = error ?? actions.error ?? eventError ?? [overview, recentQuery, sidebar, tagged, search].find((result) => result.error)?.error;

  return (
    <main className="relative grid h-full w-full grid-cols-1 grid-rows-[36px_minmax(0,1fr)_32px] gap-3 rounded-3xl bg-surface p-3 [&_button]:cursor-pointer" aria-label="表情面板"
      onPointerMoveCapture={preview.onPointerMove} onPointerLeave={preview.onPointerLeave}
      onPointerDownCapture={preview.reset} onKeyDownCapture={preview.reset} onScrollCapture={preview.onScroll} onKeyDown={(event) => {
      if (event.key !== "Escape" || composingRef.current || event.nativeEvent.isComposing || event.keyCode === 229) return;
      event.preventDefault();
      if (text) { clearSearch(); input.current?.focus(); }
      else void run(() => ipc.hidePanel());
    }}>
      <div className="flex gap-2">
        <label className="flex min-w-0 flex-1 items-center gap-2 rounded-xl bg-muted px-3 py-2 text-ink-secondary focus-within:shadow-[0_0_0_2px_var(--color-accent)] [&_svg]:shrink-0 [&_input]:w-full [&_input]:min-w-0 [&_input]:bg-transparent [&_input]:text-ink [&_input]:outline-none">
          <MagnifyingGlassIcon size={16} aria-hidden />
          <input ref={input} role="combobox" aria-label="搜索表情" aria-autocomplete="list" aria-haspopup="listbox"
            aria-expanded={!empty} aria-controls={empty ? undefined : "panel-choices"} aria-activedescendant={activeId}
            placeholder="直接输入即可搜索" value={text} autoComplete="off" spellCheck={false}
            onChange={(event) => { setText(event.target.value); setSelectedId(null); }}
            onCompositionStart={() => { composingRef.current = true; setComposing(true); }}
            onCompositionEnd={(event) => {
              setText(event.currentTarget.value); composingRef.current = false; setComposing(false); setSelectedId(null);
            }} onKeyDown={onKeyDown} />
        </label>
        <button type="button" className="grid w-9 place-items-center rounded-xl bg-muted text-ink-tertiary aria-pressed:bg-ink aria-pressed:text-surface hover:opacity-82" aria-label="固定面板" aria-pressed={pinned} disabled={pinBusy}
          onMouseDown={(event) => event.preventDefault()} onClick={() => {
            setPinBusy(true);
            void run(async () => { await ipc.setPanelPinned(!pinned); setPinned(!pinned); })
              .finally(() => setPinBusy(false));
          }}><PushPinIcon size={16} weight={pinned ? "fill" : "regular"} aria-hidden /></button>
      </div>

      <div className="flex min-h-0 flex-col gap-3">
        {empty ? <div className="flex h-full flex-1 flex-col items-center justify-center gap-2 text-ink-secondary [&_button]:text-xs [&_button]:leading-5 [&_button]:underline [&_button]:underline-offset-4">
          {restoringLibrary(sync.data) ? <p role="status">正在从同步取回表情</p> : <><p>还没有表情</p><button onClick={() => void run(ipc.showMainWindow)}>打开主窗口导入</button></>}
        </div>
          : searching ? <div id="panel-choices" className="h-full overflow-y-auto overscroll-contain" role="listbox" aria-label="搜索结果" aria-busy={!composing && !searchReady && !!text.trim()}>
            {choices.length ? <div className="grid grid-cols-4 gap-2">{choices.map((sticker) => tile(sticker, "search"))}</div>
              : <div className="flex h-full flex-1 flex-col items-center justify-center gap-2 text-ink-secondary [&_button]:text-xs [&_button]:leading-5 [&_button]:underline [&_button]:underline-offset-4"><p>{composing ? "输入完成后搜索" : !searchReady && text.trim() ? "正在搜索…" : "没有找到相关表情"}</p></div>}
          </div>
          : <>
            <section className="shrink-0" aria-labelledby="recent-heading">
              <div className="mb-2 flex justify-between px-1 text-xs text-ink-secondary [&_h2]:font-semibold [&_h2]:text-ink-tertiary"><h2 id="recent-heading">最近使用</h2><span>按数字键直接复制</span></div>
              <div id="panel-choices" role="listbox" aria-label="最近使用" className="h-41 shrink-0">
                {recent.length ? <div className="grid grid-cols-4 gap-2">{recent.map((sticker, index) => tile(sticker, "recent", index))}</div>
                  : recentQuery.isPending || overview.isPending ? recentSkeleton ? <GridSkeleton panel /> : null
                  : <div className="flex h-full flex-1 flex-col items-center justify-center gap-2 text-ink-secondary"><p>用过的表情会出现在这里</p></div>}
              </div>
            </section>
            {sidebar.isPending ? tagsSkeleton ? <><TagSkeleton panel /><GridSkeleton panel /></> : null : tags.length > 0 && <>
              <div className="flex h-8 shrink-0 items-center gap-2 overflow-x-auto overscroll-contain [scrollbar-width:thin]" aria-label="标签">
                {tags.map((tag) => <button key={tag.id} type="button" className="flex shrink-0 items-center gap-1 rounded-full border border-line px-3 py-1 text-xs font-semibold whitespace-nowrap aria-pressed:border-selection-edge aria-pressed:bg-selection aria-pressed:text-on-selection hover:opacity-82" aria-pressed={currentTag?.id === tag.id}
                  onMouseDown={(event) => event.preventDefault()} onClick={() => { setTagId(tag.id); input.current?.focus(); }}>
                  <TagDot color={tag.color} />{tag.name}
                </button>)}
              </div>
              <section className="h-41 shrink-0 overflow-y-auto overscroll-contain" aria-label={`${currentTag?.name}的表情`}>
                {tagged.isPending ? taggedSkeleton ? <GridSkeleton panel /> : null
                  : <div className="grid grid-cols-4 gap-2">{tagStickers.map((sticker) => tile(sticker, "tag"))}</div>}
              </section>
            </>}
          </>}
      </div>
      {problem && <p role="alert" className="absolute right-3 bottom-14 left-3 max-h-24 overflow-y-auto rounded-lg border border-accent bg-surface px-3 py-2 text-xs leading-5 text-accent">{String(problem)}</p>}
      <footer className="flex items-center justify-between gap-2 rounded-xl bg-chrome px-3 py-2 text-xs whitespace-nowrap text-ink-tertiary [&_button]:font-semibold [&_button:hover]:opacity-82"><span className="flex items-center gap-1">{sync.data?.target && <button type="button" title={syncTitle} aria-label={syncTitle} className="shrink-0 rounded-sm" onClick={() => void run(ipc.showSyncDetails)}><SyncIcon status={sync.data} /></button>}<span>单击复制 · 拖出发送 · Esc 收起</span></span>
        <button type="button" onMouseDown={(event) => event.preventDefault()} onClick={() => void run(ipc.showMainWindow)}>主窗口</button>
      </footer>
    </main>
  );
}

function PanelTile({ sticker, id, number, selectable, selected, copied, busy, onCopy, onDrag, preview, description }: {
  sticker: Sticker; id: string; number?: number; selectable: boolean; selected: boolean; copied: boolean; busy: boolean;
  onCopy: () => void; onDrag: (isPressed: () => boolean) => Promise<void>;
  preview: ReturnType<ReturnType<typeof usePreview>["tile"]>; description: string;
}) {
  const drag = useDragOut(onDrag, busy);
  return <button id={id} type="button" className="relative flex aspect-square min-h-0 w-full touch-none items-center justify-center rounded-lg bg-tile p-3 [transition:transform_500ms_var(--ease-standard),opacity_300ms_var(--ease-standard)] data-[selected=true]:shadow-[inset_0_0_0_2px_var(--color-accent),inset_0_0_0_4px_var(--color-surface)] hover:[transform:translateY(-2px)] active:[transform:scale(.98)] motion-reduce:transition-opacity motion-reduce:duration-300 motion-reduce:hover:transform-none motion-reduce:active:transform-none" role={selectable ? "option" : undefined}
    aria-label={sticker.name} aria-description={description} aria-selected={selectable ? selected : undefined} aria-disabled={busy}
    tabIndex={-1} data-selected={selected} onMouseDown={(event) => event.preventDefault()} onClick={onCopy} {...drag} {...preview}
    onPointerMove={(event) => { drag.onPointerMove(event); preview.onPointerMove(event); }}>
    <StickerImage path={sticker.thumbnailPath} name={sticker.name} lazy />
    {number !== undefined && <span className="absolute top-1 left-1 min-w-4 rounded-sm bg-surface px-1 text-xs font-semibold text-ink-tertiary" aria-hidden>{number}</span>}
    {sticker.animated && <span className="absolute top-1 right-1 min-w-4 rounded-sm bg-surface px-1 text-xs font-semibold text-ink-tertiary" aria-label="动图">GIF</span>}
    {copied && <span role="status" className="absolute right-1 bottom-1 left-1 rounded-sm bg-ink p-0.5 text-xs text-surface">已复制</span>}
  </button>;
}
