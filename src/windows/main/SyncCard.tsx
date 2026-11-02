import { CloudIcon } from "@phosphor-icons/react";
import type { SyncStatus } from "../../lib/ipc";
import { storageSize, syncTime } from "./useSyncStatus";

export function SyncCard({ status, onClick }: { status?: SyncStatus; onClick: () => void }) {
  const folder = status?.folder;
  const name = folder?.split(/[\\/]/).filter(Boolean).at(-1);
  return <button type="button" onClick={onClick} title={folder ?? "设置同步文件夹"}
    className="interactive mt-4 flex shrink-0 flex-col gap-2 rounded-card border border-line bg-surface p-3 text-left hover:opacity-82 active:scale-98">
    <span className="flex items-center gap-2 text-xs font-semibold"><CloudIcon size={16} className={status?.phase === "synced" ? "text-synced" : "text-ink-secondary"} />{folder ? "同步文件夹" : "在其他电脑上取用"}</span>
    {folder ? <>
      <span className="truncate text-xs" title={folder}>{name}</span>
      <span className="flex justify-between gap-2 text-xs text-ink-secondary"><span>{storageSize(status?.storageBytes ?? 0)}</span><span>{status?.onlineDevices ?? 0} 台在线</span></span>
      <span className="text-xs text-ink-secondary">上次同步 {syncTime(status?.lastSyncAt)}</span>
      {status?.phase === "error" && <span className="text-xs text-accent">同步出错，点击查看</span>}
    </> : <span className="text-xs text-ink-secondary">选择同步文件夹…</span>}
  </button>;
}
