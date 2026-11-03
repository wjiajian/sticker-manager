import { XIcon } from "@phosphor-icons/react";
import { useCallback, useRef, useState } from "react";
import { CategoryDot } from "../../components/CategoryDot";
import { ipc, type Sticker } from "../../lib/ipc";
import { useLibraryMutation } from "../../lib/queries";
import { CategoryChooser } from "./CategoryChooser";

export function StickerCategory({ sticker }: { sticker: Sticker }) {
  const mutation = useLibraryMutation(() => ipc.setStickerCategory(sticker.id, null));
  const [open, setOpen] = useState(false);
  const addButton = useRef<HTMLButtonElement>(null);
  const readOnly = sticker.deletedAt !== null;
  const category = sticker.category;
  const close = useCallback((restore = false) => { setOpen(false); if (restore) addButton.current?.focus(); }, []);
  return <section aria-label="分类" className="flex min-w-0 flex-col gap-2">
    <h3 className="text-xs font-semibold text-ink-secondary">分类</h3>
    <div className="flex flex-wrap gap-2">
      {category ? <span title={category.name} className="flex max-w-full items-center gap-1 rounded-full border border-line bg-surface px-2 py-0.5 text-xs windows:rounded-sm">
        <CategoryDot color={category.color} /><span className="truncate">{category.name}</span>
        {!readOnly && <button type="button" aria-label="移到未分类" title="移到未分类" disabled={mutation.isPending} onClick={() => mutation.mutate()} className="interactive shrink-0 rounded-full text-ink-secondary hover:opacity-82 disabled:opacity-50"><XIcon size={12} aria-hidden /></button>}
      </span> : <span className="text-xs text-ink-secondary">未分类</span>}
      {!readOnly && <button ref={addButton} type="button" aria-expanded={open} onPointerDown={(event) => event.stopPropagation()} onClick={() => setOpen(!open)} className="interactive rounded-full border border-dashed border-category-add px-2 py-0.5 text-xs text-ink-tertiary hover:opacity-82 windows:rounded-sm">{category ? "更换分类" : "设置分类"}</button>}
    </div>
    {open && !readOnly && <CategoryChooser exclude={category ? [category.id] : []} onClose={close}
      onSelect={async (chosen, name) => { if (chosen) await ipc.setStickerCategory(sticker.id, chosen.id); else await ipc.createCategoryAndSetStickers([sticker.id], name); close(true); }} />}
    {mutation.error && <p role="alert" className="text-xs text-ink-secondary">{String(mutation.error)}</p>}
  </section>;
}
