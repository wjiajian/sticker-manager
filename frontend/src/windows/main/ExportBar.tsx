import { XIcon } from "@phosphor-icons/react";
import { useState } from "react";
import { ipc } from "../../lib/ipc";
import { platform } from "../../lib/platform";
import { exportError, useExportStore } from "../../stores/export";

export function ExportBar() {
  const { status, error } = useExportStore();
  const [closed, setClosed] = useState(false);
  const [details, setDetails] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const report = status?.report;
  const problem = error ?? status?.error;
  if ((!status?.running && (!report && !problem || closed && !error))) return null;
  return <section aria-label="导出结果" className="shrink-0 border-b border-line bg-detail px-6 py-3 text-xs">
    <div className="flex items-center gap-3">
      {status?.running ? <>
        <span role="status" className="shrink-0">正在导出 {status.progress.processed}/{status.progress.total}</span>
        <progress aria-label="导出进度" max={Math.max(1, status.progress.total)} value={status.progress.total ? status.progress.processed : undefined} className="h-1 min-w-16 max-w-80 flex-1 accent-accent" />
        <button type="button" disabled={cancelling} className="shrink-0 underline underline-offset-4 disabled:opacity-50" onClick={() => { setCancelling(true); void ipc.cancelExport().catch(e => { setCancelling(false); exportError(String(e)); }); }}>{cancelling ? "正在取消…" : "取消"}</button>
      </> : <>
        <span role="status" className="min-w-0 flex-1">{report && !closed && <>
          {report.cancelled ? "已取消导出，没有生成文件" : report.output ? `已导出 ${report.exported} 张到「${report.output.split(/[/\\]/).pop()}」` : "没有导出任何表情"}
          {report.skipped > 0 && `，跳过 ${report.skipped} 张已删除的表情`}
          {report.failures.length > 0 && `，${report.failures.length} 张失败`}
        </>}</span>
        {report?.output && !closed && <button type="button" className="shrink-0 underline underline-offset-4" onClick={() => void ipc.revealExport().catch(e => exportError(String(e)))}>{platform === "macos" ? "在访达中显示" : "在资源管理器中显示"}</button>}
        {!!report?.failures.length && !closed && <button type="button" aria-expanded={details} className="shrink-0 underline underline-offset-4" onClick={() => setDetails(!details)}>{details ? "收起失败项" : `查看 ${report.failures.length} 项失败`}</button>}
        <button type="button" aria-label="关闭导出结果" title="关闭导出结果" className="shrink-0 rounded-sm p-1" onClick={() => { setClosed(true); exportError(null); }}><XIcon size={16} /></button>
      </>}
    </div>
    {details && !closed && <ul className="mt-2 max-h-40 overflow-y-auto select-text break-all text-ink-secondary">{report?.failures.map(f => <li key={f.stickerId}>{f.name}：{f.message}</li>)}</ul>}
    {report?.exceedsImportLimits && !closed && <p className="mt-1 text-ink-secondary">压缩包超过导入上限（单张 100 MB、总计 2 GB 或 10000 个条目），请先解压，再导入文件夹。</p>}
    {problem && <p role="alert" className="mt-1 text-accent">{problem}</p>}
  </section>;
}
