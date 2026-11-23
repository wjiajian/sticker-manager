import { create } from "zustand";
import { open, save } from "@tauri-apps/plugin-dialog";
import { ipc, type ExportFormat, type ExportScope, type ExportStatus } from "../lib/ipc";
import { useStatusEvent } from "../lib/useStatusEvent";

export const useExportStore = create<{ status: ExportStatus | null; choosing: boolean; error: string | null }>(() => ({
  status: null, choosing: false, error: null,
}));
const receive = (status: ExportStatus) => useExportStore.setState({ status });
export const exportError = (error: string | null) => useExportStore.setState({ error });
export function useExportEvents() {
  useStatusEvent("export-status-changed", ipc.exportStatus, receive, exportError);
}
export function exportBusy(state: ReturnType<typeof useExportStore.getState>) { return state.choosing || !!state.status?.running; }

function defaultName(scope: ExportScope) {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  const day = `${now.getFullYear()}-${pad(now.getMonth()+1)}-${pad(now.getDate())}`;
  return scope === "library" ? `表情仓库 ${day}` : `表情导出 ${day} ${pad(now.getHours())}.${pad(now.getMinutes())}`;
}

export async function startExport(scope: ExportScope, format: ExportFormat, name = defaultName(scope)) {
  if (exportBusy(useExportStore.getState())) return;
  useExportStore.setState({ choosing: true, error: null });
  try {
    const destination = format === "folder"
      ? await open({ title: "选择导出位置", directory: true, multiple: false, canCreateDirectories: true })
      : await save({ title: "导出为 .zip", defaultPath: name.replace(/[<>:"/\\|?*]/g, "_") + ".zip", filters: [{ name: "ZIP 压缩包", extensions: ["zip"] }], canCreateDirectories: true });
    if (destination) await ipc.exportStickers(scope, format, destination, name);
  } catch (reason) { exportError(String(reason)); }
  finally { useExportStore.setState({ choosing: false }); }
}
export function exportMenuItems(scope: ExportScope, name?: string) {
  return (["folder", "zip"] as const).map(format => ({
    text: format === "folder" ? "导出到文件夹…" : "导出为 .zip…",
    action: () => { void startExport(scope, format, name); },
  }));
}
