import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc, type UpdateStatus } from "../../lib/ipc";
import { storageSize } from "./useSyncStatus";

export function useUpdate() {
  const client = useQueryClient();
  const status = useQuery({ queryKey: ["update"], queryFn: ipc.updateStatus, enabled: isTauri() });
  const [dialogOpen, setDialogOpen] = useState(false);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const changed = listen<UpdateStatus>("update-status", ({ payload }) => client.setQueryData(["update"], payload));
    // 托盘的“更新到 x.y.z…”；监听后再取 pending，覆盖主窗口还没载入完时点的情况。
    const open = listen("open-update", () => { if (!disposed) setDialogOpen(true); void ipc.takeUpdateRequest().catch(() => {}); });
    void changed.then(() => client.invalidateQueries({ queryKey: ["update"] })).catch(() => {});
    void open.then(() => ipc.takeUpdateRequest()).then((pending) => { if (!disposed && pending) setDialogOpen(true); }).catch(() => {});
    return () => { disposed = true; for (const listener of [changed, open]) void listener.then((unlisten) => unlisten(), () => {}); };
  }, [client]);
  return { status: status.data, dialogOpen, showDialog: () => setDialogOpen(true), closeDialog: () => setDialogOpen(false) };
}

/** WebKit 点按钮不会聚焦它；先聚焦，关掉更新对话框后焦点才回到这个按钮，而不是落在外层对话框上。 */
export function focusOpener(button: HTMLButtonElement) {
  button.focus({ preventScroll: true });
}

export function downloadLabel(status: UpdateStatus) {
  if (status.phase === "installing") return "正在安装，完成后自动重新启动";
  if (status.total) return `正在下载 ${Math.floor((status.downloaded / status.total) * 100)}%（${storageSize(status.downloaded)} / ${storageSize(status.total)}）`;
  return status.downloaded ? `正在下载 ${storageSize(status.downloaded)}` : "正在下载";
}

/** 更新说明来自 CHANGELOG，去掉加粗、代码和链接的 Markdown 记号，按纯文本显示。 */
export function plainNotes(notes: string) {
  return notes
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/^- /gm, "• ")
    .trim();
}

export function releaseDate(value: number | null) {
  return value ? new Date(value).toLocaleDateString("zh-CN", { year: "numeric", month: "long", day: "numeric" }) : null;
}
