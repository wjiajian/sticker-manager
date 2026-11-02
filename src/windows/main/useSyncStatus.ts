import { useEffect } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc, type SyncStatus } from "../../lib/ipc";

export function useSyncStatus() {
  const client = useQueryClient();
  const result = useQuery({ queryKey: ["sync"], queryFn: ipc.syncStatus, refetchInterval: 60_000 });
  useEffect(() => {
    if (!isTauri()) return;
    const status = listen<SyncStatus>("sync-status-changed", ({ payload }) => client.setQueryData(["sync"], payload));
    const changed = listen("library-changed", () => { void client.invalidateQueries({ queryKey: ["sync"] }); });
    void status.then(() => client.invalidateQueries({ queryKey: ["sync"] })).catch(() => {});
    return () => { for (const listener of [status, changed]) void listener.then((unlisten) => unlisten(), () => {}); };
  }, [client]);
  return result;
}
export function syncLabel(status?: SyncStatus) {
  if (status?.phase === "syncing") return "同步中";
  if (status?.phase === "synced") return "已同步";
  if (status?.phase === "error") return "同步出错";
  return "未设置同步";
}
export function waitingNote(status?: SyncStatus) {
  return status?.waiting ? `还有 ${status.waiting} 项改动在等同步盘送达原图或其他电脑的记录，到了会自动应用。` : null;
}
export function syncTime(value: number | null | undefined) {
  return value ? new Date(value).toLocaleString("zh-CN", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" }) : "尚未同步";
}
export function storageSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
