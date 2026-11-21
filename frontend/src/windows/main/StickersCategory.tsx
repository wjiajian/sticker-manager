import { useCallback, useRef, useState } from "react";
import { ipc, type Sticker } from "../../lib/ipc";
import { useLibraryMutation } from "../../lib/queries";
import { CategoryChooser } from "./CategoryChooser";

/** 多选时整批设置分类。 */
export function StickersCategory({ stickers }: { stickers: Sticker[] }) {
  const ids = stickers.map((sticker) => sticker.id);
  const mutation = useLibraryMutation(() => ipc.setStickersCategory(ids, null));
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const close = useCallback((restore = false) => { setOpen(false); if (restore) button.current?.focus(); }, []);
  const anyCategorized = stickers.some((sticker) => sticker.category !== null);
  return <section aria-label="分类" className="flex w-full min-w-0 flex-col gap-2 text-left">
    <div className="flex flex-wrap justify-center gap-2">
      <button ref={button} type="button" aria-expanded={open} onPointerDown={(event) => event.stopPropagation()} onClick={() => setOpen(!open)} className="interactive rounded-full border border-dashed border-category-add px-2 py-0.5 text-xs text-ink-tertiary hover:opacity-82 windows:rounded-sm">设置分类</button>
      {anyCategorized && <button type="button" disabled={mutation.isPending} onClick={() => mutation.mutate()} className="interactive rounded-full border border-line px-2 py-0.5 text-xs text-ink-secondary hover:opacity-82 disabled:opacity-50 windows:rounded-sm">移到未分类</button>}
    </div>
    {open && <CategoryChooser onClose={close}
      onSelect={async (chosen, name) => { if (chosen) await ipc.setStickersCategory(ids, chosen.id); else await ipc.createCategoryAndSetStickers(ids, name); close(true); }} />}
    {mutation.error && <p role="alert" className="text-xs text-ink-secondary">{String(mutation.error)}</p>}
  </section>;
}