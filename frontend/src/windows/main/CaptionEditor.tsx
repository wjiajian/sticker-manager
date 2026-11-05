import { useRef } from "react";
import { ipc, type Sticker } from "../../lib/ipc";
import { useLibraryMutation } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";

export function CaptionEditor({ sticker }: { sticker: Sticker }) {
  const draft = useUiStore((state) => state.captionDrafts[sticker.id]);
  const setDraft = useUiStore((state) => state.setCaptionDraft);
  const working = useRef(false);
  const mutation = useLibraryMutation((caption: string) => ipc.setStickerCaption(sticker.id, caption));
  const readOnly = sticker.deletedAt !== null;

  async function save() {
    if (working.current || readOnly || draft === undefined) return;
    if (draft.trim() === (sticker.caption ?? "")) {
      setDraft(sticker.id, null);
      return;
    }
    working.current = true;
    try {
      await mutation.mutateAsync(draft);
      if (useUiStore.getState().captionDrafts[sticker.id] === draft) setDraft(sticker.id, null);
    } catch {
      // 草稿保留在界面状态中，切换表情后仍可回来重试。
    } finally {
      working.current = false;
    }
  }

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <label className="flex min-w-0 flex-col gap-2">
        <span className="text-xs font-semibold text-ink-secondary">配文</span>
        <input
          value={readOnly ? sticker.caption ?? "" : draft ?? sticker.caption ?? ""}
          readOnly={readOnly || mutation.isPending}
          placeholder="添加配文，可被搜索"
          aria-invalid={!!mutation.error}
          onChange={(event) => setDraft(sticker.id, event.target.value)}
          onBlur={() => { void save(); }}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing || event.keyCode === 229) return;
            if (event.key === "Escape") {
              event.preventDefault();
              if (!working.current) { setDraft(sticker.id, null); mutation.reset(); }
            } else if (event.key === "Enter") {
              event.preventDefault();
              void save();
            }
          }}
          className="min-w-0 rounded-control border border-line bg-surface px-3 py-2"
        />
      </label>
      {mutation.error && <p role="alert" className="text-xs text-ink-secondary">配文保存失败：{String(mutation.error)}</p>}
    </div>
  );
}
