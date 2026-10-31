import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { useQueryClient } from "@tanstack/react-query";
import type { DragDropEvent } from "@tauri-apps/api/webview";
import { confirm } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { ipc, type Sticker } from "../../lib/ipc";
import { platform } from "../../lib/platform";
import { useSticker } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";

export type StickerActions = ReturnType<typeof useStickerActions>;

/** 正在拖出、或者松手后还在等放下事件的表情副本。 */
interface Outgoing {
  path: string;
  ended: boolean;
  droppedHere: boolean;
  settle?: () => void;
}

export function useStickerActions(stickers: Sticker[]) {
  const client = useQueryClient();
  const selectedId = useUiStore((s) => s.selectedStickerId);
  const selectSticker = useUiStore((s) => s.selectSticker);
  const { data: detail } = useSticker(selectedId);
  const selected = detail ?? stickers.find((sticker) => sticker.id === selectedId);
  const working = useRef(false);
  // 拖出的表情经过本窗口时，webview 也会收到拖放事件。这些事件要在事件循环里排队，比拖拽结束的回调晚到，
  // 所以松手时还不知道落在哪：再等一会儿，等到这份副本的放下事件才说明拖回了本窗口。
  const outgoing = useRef<Outgoing | null>(null);
  const [busy, setBusy] = useState(false);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const feedbackTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => () => clearTimeout(feedbackTimer.current), []);

  useEffect(() => {
    if (detail === null) selectSticker(null);
  }, [detail, selectSticker]);

  /** 拖放事件属于拖出的表情时返回 true，调用方不应再把它当成导入。 */
  const claimDragDrop = useCallback((event: DragDropEvent) => {
    const current = outgoing.current;
    if (!current) return false;
    // 拖拽进行中只可能是它自己；松手后用户可能已经在拖入别的文件，只认带着这份副本路径的事件。
    if (current.ended && !("paths" in event && event.paths.some((path) => sameExport(path, current.path)))) return false;
    if (event.type === "drop") {
      current.droppedHere = true;
      current.settle?.();
    }
    return true;
  }, []);

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

  const removeFromList = useCallback((sticker: Sticker, operation: "deleteSticker" | "restoreSticker" | "purgeSticker") => perform(async () => {
    const index = stickers.findIndex((item) => item.id === sticker.id);
    const nextId = index < 0 ? null : (stickers[index + 1] ?? stickers[index - 1])?.id ?? null;
    const focused = document.activeElement?.id === `sticker-${sticker.id}`;
    const { view, searchQuery } = useUiStore.getState();
    if (operation === "purgeSticker" && !await confirm(
      "彻底删除后无法恢复。以后再导入同一张图，会作为一张新表情，标签、配文、收藏和使用次数都不会找回。",
      { title: `彻底删除“${sticker.name}”`, kind: "warning", okLabel: "彻底删除", cancelLabel: "取消" },
    )) return;
    await ipc[operation](sticker.id);
    const current = useUiStore.getState();
    // 操作期间用户已切换列表或选中别的表情，就保留新的选择。
    if (current.selectedStickerId !== sticker.id || current.view !== view || current.searchQuery !== searchQuery) return;
    selectSticker(nextId);
    if (focused && nextId) requestAnimationFrame(() => document.getElementById(`sticker-${nextId}`)?.focus());
  }), [stickers, perform, selectSticker]);

  const deleteSticker = useCallback((sticker: Sticker) => removeFromList(sticker, "deleteSticker"), [removeFromList]);
  const restoreSticker = (sticker: Sticker) => removeFromList(sticker, "restoreSticker");
  const purgeSticker = (sticker: Sticker) => removeFromList(sticker, "purgeSticker");

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const target = event.target;
      if (!selected || selected.deletedAt !== null || event.defaultPrevented || event.isComposing || event.keyCode === 229 || event.altKey) return;
      if (target instanceof HTMLElement && (target.closest("input, textarea, select, [role='textbox'], dialog, [role='dialog'], [role='listbox']") || target.isContentEditable)) return;
      const modifier = platform === "macos" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
      const copying = modifier && event.key.toLowerCase() === "c";
      const deleting = !event.shiftKey && (platform === "macos"
        ? modifier && event.key === "Backspace"
        : event.key === "Delete" && !event.ctrlKey && !event.metaKey);
      if (!copying && !deleting) return;
      event.preventDefault();
      if (!event.repeat) void (deleting ? deleteSticker(selected) : copy(selected, event.shiftKey));
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selected, copy, deleteSticker]);

  function drag(sticker: Sticker, isPressed: () => boolean) {
    if (sticker.deletedAt !== null) return Promise.resolve();
    selectSticker(sticker.id);
    return perform(async () => {
      const file = await ipc.prepareStickerDrag(sticker.id);
      // 准备大文件时用户可能已经松开鼠标，此时不再发起系统拖拽。
      if (!isPressed()) return;
      const current: Outgoing = { path: file.path, ended: false, droppedHere: false };
      outgoing.current = current;
      let dropped = false;
      try {
        dropped = await new Promise<boolean>((resolve, reject) => {
          void startDrag({ item: [file.path], icon: file.thumbnailPath, mode: "copy" }, (event) => {
            resolve(event.result === "Dropped");
          }).catch(reject);
        });
      } finally {
        current.ended = true;
        if (!dropped || current.droppedHere) release(current);
      }
      // 等待放在 perform 之外，不挡住紧接着的下一次拖出或复制。
      if (dropped && !current.droppedHere) void settleDrop(sticker.id, current);
    }, false);
  }

  /** 松手后等迟到的放下事件：等到了是拖回本窗口；等不到才是交给了其他应用，记一次使用。 */
  async function settleDrop(id: string, current: Outgoing) {
    await new Promise<void>((resolve) => {
      current.settle = resolve;
      setTimeout(resolve, 1000);
    });
    release(current);
    if (current.droppedHere) return;
    try {
      await ipc.recordStickerUsage(id);
      await client.invalidateQueries({ queryKey: ["library"] });
    } catch (reason) {
      setError(String(reason));
    }
  }

  function release(current: Outgoing) {
    if (outgoing.current === current) outgoing.current = null;
  }

  function toggleFavorite(sticker: Sticker) {
    return perform(async () => { await ipc.toggleStickerFavorite(sticker.id); });
  }

  function reveal(sticker: Sticker) {
    return perform(() => ipc.revealSticker(sticker.id), false);
  }

  return { copy, drag, toggleFavorite, reveal, deleteSticker, restoreSticker, purgeSticker, busy, claimDragDrop, copiedId, error };
}

/** 拖放事件里的路径写法可能和导出时不同（比如 /var 与 /private/var），只比较 export-xxx/文件名 这两段。 */
function sameExport(path: string, exported: string) {
  const tail = (value: string) => value.normalize("NFC").split(/[\\/]/).slice(-2).join("/");
  return tail(path) === tail(exported);
}
