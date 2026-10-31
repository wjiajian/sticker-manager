import { MagnifyingGlassIcon, PushPinIcon } from "@phosphor-icons/react";
import { skipToken, useQuery, useQueryClient } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { TagDot } from "../../components/TagDot";
import { ipc, type Sticker } from "../../lib/ipc";
import { useLibraryChangedEvents, useLibraryOverview, useStickers, useTagSidebar } from "../../lib/queries";
import { StickerImage } from "../main/StickerImage";
import { sortStickers } from "../main/stickerList";
import { useDragOut } from "../main/useDragOut";
import { usePanelActions } from "./usePanelActions";

const EMPTY: Sticker[] = [];

export function Panel() {
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
  const tagStickers = useMemo(() => sortStickers(tagged.data ?? EMPTY, "recent"), [tagged.data]);
  const search = useQuery({
    queryKey: ["library", "search", query],
    queryFn: query ? () => ipc.searchStickers(query) : skipToken,
  });
  const searching = text.length > 0;
  const searchReady = !composing && text.trim() === query && !search.isFetching && !search.isError;
  const recent = (recentQuery.data ?? EMPTY).slice(0, 8);
  // 防抖期间不让 Enter 复制上一个词的结果。
  const choices = searching ? (searchReady ? search.data ?? EMPTY : EMPTY) : recent;
  const selected = choices.find((sticker) => sticker.id === selectedId) ?? choices[0];
  const choicePrefix = searching ? "search" : "recent";
  const activeId = selected ? `${choicePrefix}-${selected.id}` : undefined;
  const empty = overview.data?.all === 0;

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
      setText(""); setQuery(""); setSelectedId(null); setError(null);
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
      onDrag={(isPressed) => actions.drag(sticker, isPressed)} />;
  }

  const problem = error ?? actions.error ?? eventError ?? [overview, recentQuery, sidebar, tagged, search].find((result) => result.error)?.error;

  return (
    <main className="panel" aria-label="表情面板" onKeyDown={(event) => {
      if (event.key !== "Escape" || composingRef.current || event.nativeEvent.isComposing || event.keyCode === 229) return;
      event.preventDefault();
      if (text) { clearSearch(); input.current?.focus(); }
      else void run(() => ipc.hidePanel());
    }}>
      <div className="panel-toolbar">
        <label className="panel-search">
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
        <button type="button" className="panel-pin" aria-label="固定面板" aria-pressed={pinned} disabled={pinBusy}
          onMouseDown={(event) => event.preventDefault()} onClick={() => {
            setPinBusy(true);
            void run(async () => { await ipc.setPanelPinned(!pinned); setPinned(!pinned); })
              .finally(() => setPinBusy(false));
          }}><PushPinIcon size={16} weight={pinned ? "fill" : "regular"} aria-hidden /></button>
      </div>

      <div className="panel-content">
        {empty ? <div className="panel-empty"><p>还没有表情</p><button onClick={() => void run(ipc.showMainWindow)}>打开主窗口导入</button></div>
          : searching ? <div id="panel-choices" className="panel-results" role="listbox" aria-label="搜索结果" aria-busy={!composing && !searchReady && !!text.trim()}>
            {choices.length ? <div className="panel-grid">{choices.map((sticker) => tile(sticker, "search"))}</div>
              : <div className="panel-empty"><p>{composing ? "输入完成后搜索" : !searchReady && text.trim() ? "正在搜索…" : "没有找到相关表情"}</p></div>}
          </div>
          : <>
            <section className="panel-recent" aria-labelledby="recent-heading">
              <div className="panel-heading"><h2 id="recent-heading">最近使用</h2><span>按数字键直接复制</span></div>
              <div id="panel-choices" role="listbox" aria-label="最近使用" className="panel-recent-grid">
                {recent.length ? <div className="panel-grid">{recent.map((sticker, index) => tile(sticker, "recent", index))}</div>
                  : <div className="panel-empty"><p>{recentQuery.isPending ? "正在读取表情…" : "用过的表情会出现在这里"}</p></div>}
              </div>
            </section>
            {tags.length > 0 && <>
              <div className="panel-tags" aria-label="标签">
                {tags.map((tag) => <button key={tag.id} type="button" className="panel-tag" aria-pressed={currentTag?.id === tag.id}
                  onMouseDown={(event) => event.preventDefault()} onClick={() => { setTagId(tag.id); input.current?.focus(); }}>
                  <TagDot color={tag.color} />{tag.name}
                </button>)}
              </div>
              <section className="panel-tag-grid" aria-label={`${currentTag?.name}的表情`}>
                <div className="panel-grid">{tagStickers.map((sticker) => tile(sticker, "tag"))}</div>
              </section>
            </>}
          </>}
      </div>
      {problem && <p role="alert" className="panel-error">{String(problem)}</p>}
      <footer className="panel-footer"><span>单击复制 · 拖出发送 · Esc 收起</span>
        <button type="button" onMouseDown={(event) => event.preventDefault()} onClick={() => void run(ipc.showMainWindow)}>主窗口</button>
      </footer>
    </main>
  );
}

function PanelTile({ sticker, id, number, selectable, selected, copied, busy, onCopy, onDrag }: {
  sticker: Sticker; id: string; number?: number; selectable: boolean; selected: boolean; copied: boolean; busy: boolean;
  onCopy: () => void; onDrag: (isPressed: () => boolean) => Promise<void>;
}) {
  const drag = useDragOut(onDrag, busy);
  return <button id={id} type="button" className="panel-tile" role={selectable ? "option" : undefined}
    aria-label={sticker.name} aria-selected={selectable ? selected : undefined} aria-disabled={busy}
    tabIndex={-1} data-selected={selected} onMouseDown={(event) => event.preventDefault()} onClick={onCopy} {...drag}>
    <StickerImage path={sticker.thumbnailPath} name={sticker.name} lazy />
    {number !== undefined && <span className="panel-number" aria-hidden>{number}</span>}
    {sticker.animated && <span className="panel-gif" aria-label="动图">GIF</span>}
    {copied && <span role="status" className="panel-copied">已复制</span>}
  </button>;
}
