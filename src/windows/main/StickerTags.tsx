import { XIcon } from "@phosphor-icons/react";
import { useCallback, useRef, useState } from "react";
import { TagDot } from "../../components/TagDot";
import { ipc, type Sticker } from "../../lib/ipc";
import { useLibraryMutation } from "../../lib/queries";
import { TagChooser } from "./TagChooser";

export function StickerTags({ sticker }: { sticker: Sticker }) {
  const mutation = useLibraryMutation((id: string) => ipc.removeStickerTag(sticker.id, id));
  const [open, setOpen] = useState(false);
  const addButton = useRef<HTMLButtonElement>(null);
  const readOnly = sticker.deletedAt !== null;
  const close = useCallback((restore = false) => { setOpen(false); if (restore) addButton.current?.focus(); }, []);
  return <section aria-label="标签" className="flex min-w-0 flex-col gap-2">
    <h3 className="text-xs font-semibold text-ink-secondary">标签</h3>
    <div className="flex flex-wrap gap-2">
      {sticker.tags.map((tag) => <span key={tag.id} title={tag.name} className="flex max-w-full items-center gap-1 rounded-full border border-line bg-surface px-2 py-0.5 text-xs windows:rounded-sm">
        <TagDot color={tag.color} /><span className="truncate">{tag.name}</span>
        {!readOnly && <button type="button" aria-label={`移除标签“${tag.name}”`} title={`移除标签“${tag.name}”`} disabled={mutation.isPending} onClick={() => mutation.mutate(tag.id)} className="interactive shrink-0 rounded-full text-ink-secondary hover:opacity-82 disabled:opacity-50"><XIcon size={12} aria-hidden /></button>}
      </span>)}
      {!readOnly && <button ref={addButton} type="button" aria-expanded={open} onPointerDown={(event) => event.stopPropagation()} onClick={() => setOpen(!open)} className="interactive rounded-full border border-dashed border-tag-add px-2 py-0.5 text-xs text-ink-tertiary hover:opacity-82 windows:rounded-sm">添加标签</button>}
      {readOnly && !sticker.tags.length && <span className="text-xs text-ink-secondary">未打标签</span>}
    </div>
    {open && !readOnly && <TagChooser exclude={sticker.tags.map((tag) => tag.id)} onClose={close} onSelect={(tag, name) => tag ? ipc.addStickerTag(sticker.id, tag.id) : ipc.createTagAndAdd(sticker.id, name)} />}
    {mutation.error && <p role="alert" className="text-xs text-ink-secondary">{String(mutation.error)}</p>}
  </section>;
}
