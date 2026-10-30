import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { ipc, type Sticker } from "../../lib/ipc";
import { platform } from "../../lib/platform";
import { useStickers } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";

export type StickerActions = ReturnType<typeof useStickerActions>;

export function useStickerActions() {
  const client = useQueryClient();
  const view = useUiStore((s) => s.view);
  const selectedId = useUiStore((s) => s.selectedStickerId);
  const selectSticker = useUiStore((s) => s.selectSticker);
  const { data: stickers } = useStickers(view);
  const selected = stickers?.find((sticker) => sticker.id === selectedId);
  const working = useRef(false);
  const internalDrop = useRef(false);
  const onInternalDrop = useCallback(() => { internalDrop.current = true; }, []);
  const [busy, setBusy] = useState(false);
  const [draggingOut, setDraggingOut] = useState(false);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const feedbackTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => () => clearTimeout(feedbackTimer.current), []);

  const perform = useCallback(async (operation: () => Promise<void>, changesLibrary = true) => {
    if (working.current) return;
    working.current = true;
    setBusy(true);
    setError(null);
    try {
      await operation();
    } catch (reason) {
      setError(String(reason));
    } finally {
      if (changesLibrary) await client.invalidateQueries({ queryKey: ["library"] });
      working.current = false;
      setBusy(false);
    }
  }, [client]);

  const copy = useCallback((sticker: Sticker, fileOnly = false) => {
    if (sticker.deletedAt !== null) return Promise.resolve();
    return perform(async () => {
      clearTimeout(feedbackTimer.current);
      setCopiedId(null);
      await ipc.copySticker(sticker.id, fileOnly);
      setCopiedId(sticker.id);
      feedbackTimer.current = setTimeout(() => setCopiedId(null), 1600);
    });
  }, [perform]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const target = event.target;
      if (!selected || selected.deletedAt !== null || event.defaultPrevented || event.isComposing || event.altKey) return;
      if (target instanceof HTMLElement && (target.closest("input, textarea, select, [role='textbox']") || target.isContentEditable)) return;
      const modifier = platform === "macos" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
      if (!modifier || event.key.toLowerCase() !== "c") return;
      event.preventDefault();
      if (!event.repeat) void copy(selected, event.shiftKey);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selected, copy]);

  function drag(sticker: Sticker, isPressed: () => boolean) {
    if (sticker.deletedAt !== null) return Promise.resolve();
    selectSticker(sticker.id);
    return perform(async () => {
      const file = await ipc.prepareStickerDrag(sticker.id);
      // 准备大文件时用户可能已经松开鼠标，此时不再发起系统拖拽。
      if (!isPressed()) return;
      internalDrop.current = false;
      setDraggingOut(true);
      try {
        const dropped = await new Promise<boolean>((resolve, reject) => {
          void startDrag({ item: [file.path], icon: file.thumbnailPath, mode: "copy" }, (event) => {
            resolve(event.result === "Dropped");
          }).catch(reject);
        });
        if (dropped && !internalDrop.current) await ipc.recordStickerUsage(sticker.id);
      } finally {
        setDraggingOut(false);
      }
    });
  }

  function toggleFavorite(sticker: Sticker) {
    return perform(async () => { await ipc.toggleStickerFavorite(sticker.id); });
  }

  function reveal(sticker: Sticker) {
    return perform(() => ipc.revealSticker(sticker.id), false);
  }

  return { copy, drag, toggleFavorite, reveal, busy, draggingOut, onInternalDrop, copiedId, error };
}
