import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { sameExport } from "../../lib/exportPath";
import { ipc, type Sticker } from "../../lib/ipc";

export function usePanelActions() {
  const working = useRef(false);
  const [busy, setBusy] = useState(false);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const feedbackTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(feedbackTimer.current), []);

  async function perform(operation: () => Promise<void>) {
    if (working.current) return;
    working.current = true;
    setBusy(true);
    setError(null);
    try { await operation(); }
    catch (reason) { setError(String(reason)); }
    finally { working.current = false; setBusy(false); }
  }

  function copy(sticker: Sticker, key: string) {
    return perform(async () => {
      await ipc.copySticker(sticker.id, false);
      const hidden = await ipc.hidePanel(true);
      if (!hidden) {
        clearTimeout(feedbackTimer.current);
        setCopiedKey(key);
        feedbackTimer.current = setTimeout(() => setCopiedKey(null), 1600);
      }
    });
  }

  function drag(sticker: Sticker, isPressed: () => boolean) {
    return perform(async () => {
      const file = await ipc.prepareStickerDrag(sticker.id);
      if (!isPressed()) return;
      let droppedHere = false;
      let settle: (() => void) | undefined;
      // 监听所有本应用窗口，放回面板或主窗口都不算交给其他应用。
      const unlisten = await listen<{ paths: string[] }>("tauri://drag-drop", ({ payload }) => {
        if (payload.paths.some((path) => sameExport(path, file.path))) {
          droppedHere = true;
          settle?.();
        }
      });
      try {
        if (!isPressed()) return;
        // 原生拖拽会临时失焦，等系统返回结果再决定是否收起。
        await ipc.setPanelDragging(true);
        if (!isPressed()) return;
        const dropped = await new Promise<boolean>((resolve, reject) => {
          void startDrag({ item: [file.path], icon: file.thumbnailPath, mode: "copy" }, (event) => {
            resolve(event.result === "Dropped");
          }).catch(reject);
        });
        if (dropped && !droppedHere) {
          // WebView 的放下事件可能晚于原生回调，与主窗口的拖回判定等待相同。
          await new Promise<void>((resolve) => {
            const timer = setTimeout(resolve, 1000);
            settle = () => { clearTimeout(timer); resolve(); };
          });
          if (!droppedHere) {
            await ipc.recordStickerUsage(sticker.id);
            await ipc.hidePanel(true);
          }
        }
      } finally {
        unlisten();
        await ipc.setPanelDragging(false);
      }
    });
  }

  const reset = useCallback(() => {
    setError(null);
    clearTimeout(feedbackTimer.current);
    setCopiedKey(null);
  }, []);

  return { copy, drag, busy, copiedKey, error, reset };
}
