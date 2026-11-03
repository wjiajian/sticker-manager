import { XIcon } from "@phosphor-icons/react";
import type { StickerActions } from "./useStickerActions";

export function UndoBar({ actions }: { actions: StickerActions }) {
  const { undo } = actions;
  if (!undo) return null;
  return <section aria-label="删除结果" className="shrink-0 border-b border-line bg-detail px-6 py-3 text-xs">
    <div className="flex items-center gap-3">
      <span role="status" className="min-w-0 flex-1">{undo.label}</span>
      <button type="button" disabled={actions.busy} onClick={() => void actions.undoDelete()} className="shrink-0 underline underline-offset-4 disabled:opacity-50">撤销</button>
      <button type="button" aria-label="关闭" title="关闭" className="shrink-0 rounded-sm p-1" onClick={actions.dismissUndo}><XIcon size={16} /></button>
    </div>
  </section>;
}
