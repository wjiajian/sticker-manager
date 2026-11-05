import { SyncIcon } from "./SyncIcon";
import type { SyncStatus } from "../../lib/ipc";
import { storageSize, syncTime, syncLabel } from "./useSyncStatus";

export function SyncCard({ status, onClick }: { status?: SyncStatus; onClick: (button: HTMLButtonElement) => void }) {
  const target = status?.target;
  return <button type="button" aria-haspopup="dialog" onClick={(event) => onClick(event.currentTarget)} title={target ? `${target.name} · ${target.address}` : "设置同步"}
    className="interactive mt-4 flex shrink-0 flex-col gap-2 rounded-card border border-line bg-surface p-3 text-left hover:opacity-82 active:scale-98">
    <span className="flex items-center gap-2 text-xs font-semibold"><SyncIcon status={status} />{target ? `${target.name} · ${syncLabel(status)}` : "在其他电脑上取用"}</span>
    {target ? <>
      <span className="truncate text-xs" title={target.address}>{target.shortAddress}</span>
      <span className="flex justify-between gap-2 text-xs text-ink-secondary"><span>{storageSize(status?.storageBytes ?? 0)}</span><span>{status?.onlineDevices ?? 0} 台在线</span></span>
      <span className="text-xs text-ink-secondary">上次同步 {syncTime(status?.lastSyncAt)}</span>
      {status?.phase === "error" && <span className="text-xs text-accent">同步出错，点击查看</span>}
    </> : <span className="text-xs text-ink-secondary">设置同步…</span>}
  </button>;
}
