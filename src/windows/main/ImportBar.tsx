import { XIcon } from "@phosphor-icons/react";
import { useCallback, useRef, useState } from "react";
import { ipc, type ImportStatus } from "../../lib/ipc";
import { formatReport } from "../../lib/importReport";
import { TagChooser } from "./TagChooser";

export function ImportBar({ status, error, clearError }: { status: ImportStatus | null; error: string | null; clearError: () => void }) {
  const [dismissed, setDismissed] = useState(false);
  const [failures, setFailures] = useState(false);
  const [tags, setTags] = useState(false);
  const [tagResult, setTagResult] = useState("");
  const [cancelled, setCancelled] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const closeTags = useCallback((restore = false) => { setTags(false); if (restore) trigger.current?.focus(); }, []);
  const report = status?.report;
  const problem = error ?? status?.error ?? actionError;
  if ((!status?.running && dismissed && !error && !actionError) || (!status?.running && !report && !problem)) return null;
  return <section aria-label="导入结果" className="relative shrink-0 border-b border-line bg-detail px-6 py-3 text-xs">
    {status?.running ? <div className="flex items-center gap-3">
      <span role="status" className="shrink-0">正在导入 {status.progress.processed}/{status.progress.total}</span>
      <progress aria-label="导入进度" max={Math.max(1, status.progress.total)} value={status.progress.total ? status.progress.processed : undefined} className="h-1 min-w-16 max-w-80 flex-1 accent-accent" />
      <button type="button" disabled={cancelled} className="underline underline-offset-4 disabled:opacity-50" onClick={() => { setCancelled(true); void ipc.cancelImport().catch((e: unknown) => { setCancelled(false); setActionError(String(e)); }); }}>{cancelled ? "正在取消…" : "取消"}</button>
    </div> : <div className="flex items-center gap-3">
      <div role="status" className="min-w-0 flex-1">{report && !dismissed && formatReport(report)}{tagResult && <p className="mt-1">{tagResult}</p>}</div>
      {!!report?.failures.length && !dismissed && <button type="button" aria-expanded={failures} onClick={() => setFailures(!failures)} className="shrink-0 underline underline-offset-4">{failures ? "收起失败项" : `查看 ${report.failures.length} 项失败`}</button>}
      {!!report?.stickerIds.length && !dismissed && <div className="relative shrink-0">
        <button ref={trigger} type="button" aria-expanded={tags} onPointerDown={(event) => event.stopPropagation()} onClick={() => setTags(!tags)} className="underline underline-offset-4">给这批表情加标签…</button>
        {tags && <div className="absolute top-full right-0 z-30 mt-2 w-64"><TagChooser onClose={closeTags} onSelect={async (tag, name) => {
          const count = tag ? await ipc.addTagToStickers(report.stickerIds, tag.id) : (await ipc.createTagAndAddToStickers(report.stickerIds, name))[1];
          setTagResult(count ? `已给 ${count} 张加上标签「${name}」` : `这批表情都已有标签「${name}」`);
        }} /></div>}
      </div>}
      <button type="button" aria-label="关闭导入结果" title="关闭导入结果" className="shrink-0 rounded-sm p-1" onClick={() => { setDismissed(true); setTags(false); setActionError(null); clearError(); }}><XIcon size={16} /></button>
    </div>}
    {problem && <p role="alert" className="mt-1 text-accent">{problem}</p>}
    {failures && !dismissed && !status?.running && <ul className="mt-2 max-h-40 overflow-y-auto select-text break-all text-ink-secondary">{report?.failures.map((failure, index) => <li key={index}>{failure.path}：{failure.message}</li>)}</ul>}
  </section>;
}
