import { useState, useRef } from "react";
import { ipc, type Sticker } from "../../lib/ipc";
import { useLibraryMutation } from "../../lib/queries";

export function NameEditor({ sticker }: { sticker: Sticker }) {
  const [draft, setDraft] = useState<string | null>(null);
  const working = useRef(false);
  const mutation = useLibraryMutation((name: string) => ipc.setStickerName(sticker.id, name));
  async function save() {
    if (draft === null || working.current || sticker.deletedAt !== null) return;
    if (draft.trim() === sticker.name) { setDraft(null); return; }
    working.current = true;
    try { await mutation.mutateAsync(draft); setDraft(null); }
    catch { /* 保留输入，方便修正后重试。 */ }
    finally { working.current = false; }
  }
  return <div className="flex flex-col gap-1">
    <input aria-label="表情名称" value={draft ?? sticker.name}
      readOnly={sticker.deletedAt !== null || mutation.isPending} aria-invalid={!!mutation.error}
      className="min-w-0 rounded-control border border-transparent bg-transparent text-lg font-semibold focus:border-line focus:bg-surface"
      onChange={(event) => setDraft(event.target.value)} onBlur={() => { void save(); }}
      onKeyDown={(event) => {
        if (event.nativeEvent.isComposing || event.keyCode === 229) return;
        if (event.key === "Enter") { event.preventDefault(); void save(); }
        if (event.key === "Escape" && !working.current) { event.preventDefault(); setDraft(null); mutation.reset(); }
      }} />
    {mutation.error && <p role="alert" className="text-xs text-ink-secondary">名称保存失败：{String(mutation.error)}</p>}
  </div>;
}
