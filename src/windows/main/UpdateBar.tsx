import { XIcon } from "@phosphor-icons/react";
import { useState } from "react";
import { ipc, type UpdateStatus } from "../../lib/ipc";
import { downloadLabel, focusOpener } from "./useUpdate";

/** 自动或手动检查发现新版本时显示；忽略过的版本只在正在下载或安装时显示。 */
export function UpdateBar({ status, onShow }: { status?: UpdateStatus; onShow: () => void }) {
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const version = status?.available?.version;
  const working = status?.phase === "downloading" || status?.phase === "installing";
  if (!status || !version || (!working && (status.ignored || dismissed === version))) return null;
  return <section aria-label="新版本" className="shrink-0 border-b border-line bg-detail px-6 py-3 text-xs">
    <div className="flex items-center gap-3">
      <span role="status" className="min-w-0 flex-1">{working ? `新版本 ${version}：${downloadLabel(status)}` : `新版本 ${version} 可用`}</span>
      <button type="button" onClick={(event) => { focusOpener(event.currentTarget); onShow(); }} className="shrink-0 underline underline-offset-4">查看</button>
      {!working && <>
        <button type="button" onClick={() => { setError(null); void ipc.ignoreUpdate(version).catch((e: unknown) => setError(String(e))); }}
          className="shrink-0 underline underline-offset-4">忽略这个版本</button>
        <button type="button" aria-label="关闭" title="关闭" className="shrink-0 rounded-sm p-1" onClick={() => setDismissed(version)}><XIcon size={16} /></button>
      </>}
    </div>
    {(error ?? status.error) && <p role="alert" className="mt-1 text-accent">{error ?? status.error}</p>}
  </section>;
}
