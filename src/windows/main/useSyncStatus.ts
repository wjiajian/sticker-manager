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
  if (status?.phase === "paused") return "已暂停";
  if (status?.phase === "synced") return "已同步";
  if (status?.phase === "error") return "同步出错";
  return "未设置同步";
}
export function waitingNote(status?: SyncStatus) {
  return status?.waiting ? `还有 ${status.waiting} 项改动在等原图或其他电脑的记录送达，到了会自动应用。` : null;
}
export function syncTime(value: number | null | undefined) {
  return value ? new Date(value).toLocaleString("zh-CN", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" }) : "尚未同步";
}
export function storageSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function progressLabel(status?: SyncStatus) {
  const progress = status?.progress;
  if (!progress || status?.phase !== "syncing") return null;
  return progress.total === null ? `本轮已下载 ${progress.completed} 张` : `正在${progress.stage === "uploading" ? "上传" : "下载"}原图 ${progress.completed}/${progress.total}`;
}
// 只在真的在同步时显示“正在取回”；第一次同步出错或暂停时回到导入说明，不把空仓库卡在等待里。
export function restoringLibrary(status?: SyncStatus) {
  return !!status?.target && status.phase === "syncing" && (status.lastSyncAt === null || status.progress?.stage === "downloading");
}
export function syncTooltip(status?: SyncStatus) {
  return [syncStateLabel(status), status?.target && `${status.target.name} · ${status.target.shortAddress}`, progressLabel(status), status?.error ?? waitingNote(status)]
    .filter(Boolean).join("\n") || "打开同步偏好设置";
}

export function syncStateLabel(status?: SyncStatus) {
  return status?.phase === "paused" ? `已暂停 · ${status.pauseReason === "metered" ? "正在使用按流量计费的网络" : "手动暂停"}` : syncLabel(status);
}
export function deviceTime(value: number | null | undefined, now = Date.now()) {
  if (!value) return "尚未同步";
  const elapsed = Math.max(0, now - value);
  if (elapsed < 60_000) return "刚刚";
  if (elapsed < 3_600_000) return `${Math.floor(elapsed / 60_000)} 分钟前`;
  const date = new Date(value), today = new Date(now);
  const start = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const yesterday = new Date(start); yesterday.setDate(yesterday.getDate() - 1);
  const time = `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
  if (value >= start.getTime()) return `今天 ${time}`;
  if (value >= yesterday.getTime()) return `昨天 ${time}`;
  return `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()}`;
}
