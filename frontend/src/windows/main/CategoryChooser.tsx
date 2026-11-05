import { useEffect, useId, useRef, useState } from "react";
import { CategoryDot } from "../../components/CategoryDot";
import type { CategorySummary } from "../../lib/ipc";
import { useCategories, useLibraryMutation } from "../../lib/queries";
import { normalizeCategoryName } from "../../lib/categoryColors";
import { CategorySkeleton, useDelayedLoading } from "../../components/Skeleton";

export function CategoryChooser({ exclude = [], onSelect, onClose }: { exclude?: string[]; onSelect: (category: CategorySummary | null, name: string) => Promise<unknown>; onClose: (restore?: boolean) => void }) {
  const { data: categories, error, isPending } = useCategories();
  const skeleton = useDelayedLoading(isPending);
  const mutation = useLibraryMutation(({ category, name }: { category: CategorySummary | null; name: string }) => onSelect(category, name));
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const working = useRef(false);
  const listId = useId();
  const all = categories ?? [];
  const normalized = normalizeCategoryName(query);
  const options: { category: CategorySummary | null; name: string }[] = all.filter((category) => !exclude.includes(category.id) && normalizeCategoryName(category.name).includes(normalized)).map((category) => ({ category, name: category.name }));
  if (normalized && categories && !all.some((category) => normalizeCategoryName(category.name) === normalized)) options.push({ category: null, name: query.trim() });
  const activeIndex = Math.min(active, options.length - 1);
  useEffect(() => { input.current?.focus(); }, []);
  useEffect(() => {
    function outside(event: PointerEvent) { if (event.target instanceof Node && !root.current?.contains(event.target)) onClose(); }
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [onClose]);
  useEffect(() => { if (activeIndex >= 0) document.getElementById(`${listId}-${activeIndex}`)?.scrollIntoView({ block: "nearest" }); }, [activeIndex, listId]);
  async function choose(index: number) {
    const option = options[index];
    if (!option || working.current) return;
    working.current = true;
    try { await mutation.mutateAsync(option); setQuery(""); setActive(0); input.current?.focus(); }
    catch { /* 保留输入供重试，错误显示在下面。 */ }
    finally { working.current = false; }
  }
  return <div ref={root} className="flex min-w-0 flex-col gap-1 rounded-control border border-line bg-surface p-2"
    onBlur={(event) => { if (event.relatedTarget instanceof Node && !event.currentTarget.contains(event.relatedTarget)) onClose(); }}>
    <input ref={input} role="combobox" aria-label="筛选或新建分类" aria-autocomplete="list" aria-expanded="true" aria-controls={listId}
      aria-activedescendant={activeIndex >= 0 ? `${listId}-${activeIndex}` : undefined} value={query} readOnly={mutation.isPending} placeholder="筛选或新建分类"
      onChange={(event) => { setQuery(event.target.value); setActive(0); }}
      onKeyDown={(event) => {
        if (event.nativeEvent.isComposing || event.keyCode === 229) return;
        if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); onClose(true); }
        else if (event.key === "Enter") { event.preventDefault(); void choose(activeIndex); }
        else if (event.key === "ArrowDown" || event.key === "ArrowUp") { event.preventDefault(); if (options.length) setActive((activeIndex + (event.key === "ArrowDown" ? 1 : -1) + options.length) % options.length); }
      }} className="w-full min-w-0 rounded-control border border-line px-2 py-1 text-xs" />
    <div id={listId} role="listbox" aria-label="可选的分类" aria-busy={isPending || mutation.isPending} className="max-h-48 overflow-y-auto">
      {options.map((option, index) => <button key={option.category?.id ?? "create"} id={`${listId}-${index}`} type="button" role="option" aria-selected={index === activeIndex} tabIndex={-1}
        title={option.name} disabled={mutation.isPending} onPointerDown={(event) => event.preventDefault()} onMouseMove={() => setActive(index)} onClick={() => void choose(index)}
        className={`interactive flex w-full min-w-0 items-center gap-2 rounded-control px-2 py-1 text-left text-xs hover:bg-hover ${index === activeIndex ? "bg-hover" : ""}`}>
        {option.category && <CategoryDot color={option.category.color} />}<span className="truncate">{option.category ? option.name : `新建并放入“${option.name}”`}</span>
      </button>)}
    </div>
    {isPending ? skeleton ? <CategorySkeleton /> : null : error ? <p role="alert" className="text-xs text-accent">{String(error)}</p> : !options.length && <p className="text-xs text-ink-secondary">{normalized ? "没有更多匹配的分类" : "输入名称可新建分类"}</p>}
    {mutation.error && <p role="alert" className="text-xs text-accent">{String(mutation.error)}</p>}
  </div>;
}
