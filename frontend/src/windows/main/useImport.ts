import { isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { ipc, type ImportStatus, type ExternalDropEvent } from "../../lib/ipc";
import { isEditableTarget, isPasteShortcut } from "../../lib/pasteShortcut";
import { platform } from "../../lib/platform";
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
      if (!disposed) { setStatus(payload); if (payload.running && payload.progress.processed === 0) { reset(); setNotice(null); setDialogError(null); } }
    });
    void listening.then(() => ipc.importStatus()).then((value) => { if (!disposed && !seenEvent) setStatus(value); }).catch((reason: unknown) => { if (!disposed) setDialogError(String(reason)); });
    return () => { disposed = true; void listening.then((cleanup) => cleanup(), () => {}); };
  }, [reset]);
  const [choosing, setChoosing] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [receiving, setReceiving] = useState(false);
  const [pasting, setPasting] = useState(false);
  const pasteInProgress = useRef(false);
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
        setNotice(null);
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

  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(null), 4000);
    return () => clearTimeout(timer);
  }, [notice]);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const listening = listen<ExternalDropEvent>("external-drop", ({ payload }) => {
      if (disposed) return;
      if (payload.type === "enter") { setDragging(true); setNotice(null); setDialogError(null); }
      else if (payload.type === "leave") setDragging(false);
      else if (payload.type === "receiving") { setReceiving(true); setDragging(false); setNotice(null); setDialogError(null); }
      else if (payload.type === "receivingDone") setReceiving(false);
      else if (payload.type === "rejected") { setDragging(false); setNotice(payload.message); }
      else if (payload.type === "failed") { setReceiving(false); setDragging(false); setDialogError(payload.message); }
    });
    void listening.catch((reason: unknown) => { if (!disposed) setDropError(String(reason)); });
    return () => { disposed = true; void listening.then((cleanup) => cleanup(), () => {}); };
  }, []);
  const busy = choosing || pasting || receiving || !!status?.running;
  const paste = useCallback(async () => {
    if (busy || pasteInProgress.current) return;
    pasteInProgress.current = true;
    setPasting(true);
    setNotice(null);
    setDialogError(null);
    reset();
    try {
      const result = await ipc.importClipboard();
      if (result.kind === "empty") setNotice(result.message);
    } catch (reason) {
      setDialogError(String(reason));
    } finally {
      pasteInProgress.current = false;
      setPasting(false);
    }
  }, [busy, reset]);
  useEffect(() => {
    if (!isTauri()) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.defaultPrevented || event.isComposing || event.keyCode === 229 || document.querySelector("dialog[open]") || isEditableTarget(event.target) || !isPasteShortcut(event, platform)) return;
      event.preventDefault();
      if (!event.repeat) void paste();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [paste]);

  async function choose(directory: boolean) {
    setChoosing(true);
    setNotice(null);
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
    choose, paste,
    dragging,
    busy,
    status,
    error: dialogError ?? (error ? String(error) : null),
    clearError: () => { setDialogError(null); reset(); },
    message: [dragging ? "松开以导入表情" : "", receiving ? "正在接收拖入的图片…" : "", pasting ? "正在读取剪贴板…" : "", notice, dropError].filter(Boolean).join("\n"),
  };
}
