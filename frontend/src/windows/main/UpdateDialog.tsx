import { useEffect, useId, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { ipc, type UpdateStatus } from "../../lib/ipc";
import { downloadLabel, plainNotes, releaseDate } from "./useUpdate";

/** 显示新版本说明；确认后下载、安装并自动重启。关掉对话框不会中断已经开始的下载。 */
export function UpdateDialog({ status, onClose }: { status?: UpdateStatus; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const element = dialog.current!;
    element.showModal();
    return () => element.close();
  }, []);
  const available = status?.available;
  const working = status?.phase === "downloading" || status?.phase === "installing";
  const date = releaseDate(available?.date ?? null);
  const problem = status?.error ?? error;
  function install() {
    setError(null);
    // 失败原因也会记进更新状态；这里兜住开始前就被拒绝的情况。
    void ipc.installUpdate().catch((e: unknown) => setError(String(e)));
  }
  return <dialog ref={dialog} aria-labelledby={titleId} onCancel={(event) => { event.preventDefault(); onClose(); }}
    className="m-auto max-h-[calc(100%-48px)] w-120 max-w-[calc(100%-48px)] overflow-y-auto rounded-card border border-line bg-surface p-6 text-ink backdrop:bg-overlay">
    <div className="flex flex-col gap-4">
      <h2 id={titleId} className="text-lg font-semibold">{available ? `新版本 ${available.version}` : "检查更新"}</h2>
      {available ? <>
        <p className="text-xs text-ink-secondary">当前版本 {status.currentVersion}{date && ` · ${date}发布`}</p>
        {available.notes
          ? <div tabIndex={0} aria-label="更新说明" className="max-h-64 overflow-y-auto rounded-control bg-muted p-3 text-xs whitespace-pre-wrap select-text">{plainNotes(available.notes)}</div>
          : <p className="text-xs text-ink-secondary">这个版本没有附更新说明。</p>}
        {working
          ? <div className="flex flex-col gap-2">
            <span role="status" className="text-xs">{downloadLabel(status)}</span>
            <progress aria-label="更新进度" max={status.total ?? undefined} value={status.phase === "downloading" && status.total ? status.downloaded : undefined} className="h-1 w-full accent-accent" />
          </div>
          : <p className="text-xs text-ink-secondary">安装完成后应用会自动重新启动。</p>}
      </> : <p role="status" className="text-xs text-ink-secondary">{status?.phase === "checking" ? "正在检查…" : "已是最新版本"}</p>}
      {problem && <p role="alert" className="text-xs text-accent">{problem}</p>}
      <div className="flex justify-end gap-2">
        <Button onClick={onClose}>{available && !working ? "以后再说" : "关闭"}</Button>
        {available && <Button variant="primary" disabled={working || status.phase === "checking"} onClick={install}>{working ? "正在更新…" : "下载并安装"}</Button>}
      </div>
    </div>
  </dialog>;
}
