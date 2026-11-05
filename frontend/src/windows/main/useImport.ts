import { isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { ipc, type ImportStatus } from "../../lib/ipc";
import { useImportStickers } from "../../lib/queries";

export function useImport(claimDragDrop: (event: DragDropEvent) => boolean) {
  const { mutate, error, reset } = useImportStickers();
  const [status, setStatus] = useState<ImportStatus | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let seenEvent = false;
    const listening = listen<ImportStatus>("import-status-changed", ({ payload }) => {
      seenEvent = true;
      if (!disposed) { setStatus(payload); if (payload.running && payload.progress.processed === 0) reset(); }
    });
    void listening.then(() => ipc.importStatus()).then((value) => { if (!disposed && !seenEvent) setStatus(value); }).catch((reason: unknown) => { if (!disposed) setDialogError(String(reason)); });
    return () => { disposed = true; void listening.then((cleanup) => cleanup(), () => {}); };
  }, [reset]);
  const [choosing, setChoosing] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [dropError, setDropError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (disposed) return;
      if (claimDragDrop(payload)) {
        setDragging(false);
        return;
      }
      setDragging(payload.type === "enter" || payload.type === "over");
      if (payload.type === "drop" && payload.paths.length > 0) {
        setDialogError(null);
        mutate(payload.paths);
      }
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    }).catch((reason: unknown) => {
      if (!disposed) setDropError(`无法启用拖入导入：${String(reason)}`);
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [mutate, claimDragDrop]);

  async function choose(directory: boolean) {
    setChoosing(true);
    setDialogError(null);
    try {
      const paths = await open({
        title: directory ? "导入文件夹" : "导入表情",
        multiple: true,
        directory,
        filters: directory ? undefined : [{ name: "表情", extensions: ["png", "jpg", "jpeg", "webp", "gif", "apng", "zip"] }],
      });
      if (paths && paths.length > 0) mutate(paths);
    } catch (reason) {
      setDialogError(`无法打开选择框：${String(reason)}`);
    } finally {
      setChoosing(false);
    }
  }

  return {
    choose,
    dragging,
    busy: choosing || !!status?.running,
    status,
    error: dialogError ?? (error ? String(error) : null),
    clearError: () => { setDialogError(null); reset(); },
    message: [dragging ? "松开以导入表情" : "", dropError].filter(Boolean).join("\n"),
  };
}
