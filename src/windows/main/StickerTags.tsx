import { XIcon } from "@phosphor-icons/react";
import { useEffect, useId, useRef, useState } from "react";
import { TagDot } from "../../components/TagDot";
import { ipc, type Sticker, type TagSummary } from "../../lib/ipc";
import { useLibraryMutation, useTagSidebar } from "../../lib/queries";
import { normalizeTagName } from "../../lib/tagColors";

export function StickerTags({ sticker }: { sticker: Sticker }) {
  const { data: sidebar, error: queryError, isPending } = useTagSidebar();
  const mutation = useLibraryMutation((operation: () => Promise<unknown>) => operation());
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const chooser = useRef<HTMLDivElement>(null);
  const addButton = useRef<HTMLButtonElement>(null);
  const working = useRef(false);
  const opened = useRef(false);
  const listId = useId();
  const readOnly = sticker.deletedAt !== null;
  const tags = [...(sidebar?.groups.flatMap((group) => group.tags) ?? []), ...(sidebar?.ungrouped ?? [])];
  const normalized = normalizeTagName(query);
  const options: { tag: TagSummary | null; name: string }[] = tags
    .filter((tag) => !sticker.tags.some((added) => added.id === tag.id) && normalizeTagName(tag.name).includes(normalized))
    .map((tag) => ({ tag, name: tag.name }));
  if (normalized && sidebar && !tags.some((tag) => normalizeTagName(tag.name) === normalized)) {
    options.push({ tag: null, name: query.trim() });
  }
  const activeIndex = Math.min(active, options.length - 1);

  function close(restoreFocus = false) {
    opened.current = false;
    setOpen(false);
    if (restoreFocus) addButton.current?.focus();
  }

  useEffect(() => {
    if (!open) return;
    input.current?.focus();
    function outside(event: PointerEvent) {
      if (event.target instanceof Node && !chooser.current?.contains(event.target) && !addButton.current?.contains(event.target)) close();
    }
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);

  useEffect(() => {
    if (open && activeIndex >= 0) document.getElementById(`${listId}-${activeIndex}`)?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, open, listId]);

  async function update(operation: () => Promise<unknown>, adding = false) {
    if (working.current || readOnly) return;
    working.current = true;
    try {
      await mutation.mutateAsync(operation);
      if (adding) {
        setQuery("");
        setActive(0);
        if (opened.current) input.current?.focus();
      }
    } catch {
      // 错误显示在标签区，输入保留以便重试。
    } finally {
      working.current = false;
    }
  }

  function choose(index: number) {
    const option = options[index];
    if (!option) return;
    void update(() => option.tag ? ipc.addStickerTag(sticker.id, option.tag.id) : ipc.createTagAndAdd(sticker.id, option.name), true);
  }

  return (
    <section aria-label="标签" className="flex min-w-0 flex-col gap-2">
      <h3 className="text-xs font-semibold text-ink-secondary">标签</h3>
      <div className="flex flex-wrap gap-2">
        {sticker.tags.map((tag) => (
          <span key={tag.id} title={tag.name} className="flex max-w-full items-center gap-1 rounded-full border border-line bg-surface px-2 py-0.5 text-xs windows:rounded-sm">
            <TagDot color={tag.color} />
            <span className="truncate">{tag.name}</span>
            {!readOnly && <button
              type="button"
              aria-label={`移除标签“${tag.name}”`}
              title={`移除标签“${tag.name}”`}
              disabled={mutation.isPending}
              onClick={() => { void update(() => ipc.removeStickerTag(sticker.id, tag.id)); }}
              className="interactive shrink-0 rounded-full text-ink-secondary hover:opacity-82 active:scale-98 disabled:opacity-50 windows:rounded-xs"
            ><XIcon size={12} aria-hidden /></button>}
          </span>
        ))}
        {!readOnly && <button
          ref={addButton}
          type="button"
          aria-expanded={open}
          aria-controls={open ? listId : undefined}
          onClick={() => {
            if (open) close();
            else { opened.current = true; setQuery(""); setActive(0); setOpen(true); }
          }}
          className="interactive rounded-full border border-dashed border-tag-add bg-transparent px-2 py-0.5 text-xs text-ink-tertiary hover:opacity-82 active:scale-98 windows:rounded-sm"
        >添加标签</button>}
        {readOnly && sticker.tags.length === 0 && <span className="text-xs text-ink-secondary">未打标签</span>}
      </div>
      {open && !readOnly && <div
        ref={chooser}
        className="flex min-w-0 flex-col gap-1 rounded-control border border-line bg-surface p-2"
        onBlur={(event) => { if (event.relatedTarget instanceof Node && !event.currentTarget.contains(event.relatedTarget)) close(); }}
      >
        <input
          ref={input}
          role="combobox"
          aria-label="筛选或新建标签"
          aria-autocomplete="list"
          aria-expanded="true"
          aria-controls={listId}
          aria-activedescendant={activeIndex >= 0 ? `${listId}-${activeIndex}` : undefined}
          value={query}
          readOnly={mutation.isPending}
          placeholder="筛选或新建标签"
          onChange={(event) => { setQuery(event.target.value); setActive(0); }}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing || event.keyCode === 229) return;
            if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(true); }
            else if (event.key === "Enter") { event.preventDefault(); choose(activeIndex); }
            else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              if (options.length) setActive((activeIndex + (event.key === "ArrowDown" ? 1 : -1) + options.length) % options.length);
            }
          }}
          className="w-full min-w-0 rounded-control border border-line px-2 py-1 text-xs"
        />
        <div id={listId} role="listbox" aria-label="可添加的标签" aria-busy={isPending || mutation.isPending} className="max-h-48 overflow-y-auto">
          {options.map((option, index) => (
            <button
              key={option.tag?.id ?? "create"}
              id={`${listId}-${index}`}
              type="button"
              role="option"
              aria-selected={index === activeIndex}
              tabIndex={-1}
              title={option.tag ? option.name : `新建并添加“${option.name}”`}
              disabled={mutation.isPending}
              onPointerDown={(event) => event.preventDefault()}
              onMouseMove={() => setActive(index)}
              onClick={() => choose(index)}
              className={`interactive flex w-full min-w-0 items-center gap-2 rounded-control px-2 py-1 text-left text-xs hover:bg-hover active:scale-98 ${index === activeIndex ? "bg-hover" : ""}`}
            >
              {option.tag && <TagDot color={option.tag.color} />}
              <span className="truncate">{option.tag ? option.name : `新建并添加“${option.name}”`}</span>
            </button>
          ))}
        </div>
        {isPending ? <p role="status" className="text-xs text-ink-secondary">正在读取标签…</p>
          : queryError ? <p role="alert" className="text-xs text-ink-secondary">{String(queryError)}</p>
          : options.length === 0 && <p role="status" className="text-xs text-ink-secondary">{normalized ? "这个标签已添加" : "输入名称可新建标签"}</p>}
      </div>}
      {mutation.error && <p role="alert" className="text-xs text-ink-secondary">{String(mutation.error)}</p>}
    </section>
  );
}
