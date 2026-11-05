import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { useQueryClient } from "@tanstack/react-query";
import type { DragDropEvent } from "@tauri-apps/api/webview";
import { confirm } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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

/** 删除后可以撤销的时长。 */
const UNDO_MS = 10_000;

export function useStickerActions(stickers: Sticker[]) {
  const client = useQueryClient();
  const selectedId = useUiStore((s) => s.selectedStickerId);
  const selectedIds = useUiStore((s) => s.selectedStickerIds);
  const selectSticker = useUiStore((s) => s.selectSticker);
  const setSelection = useUiStore((s) => s.setSelection);
  const { data: detail } = useSticker(selectedId);
  const selected = detail ?? stickers.find((sticker) => sticker.id === selectedId);
  // 多选时当前列表里被选中的全部表情；单选或没选时为空。
  const selectedStickers = useMemo(
    () => selectedIds.length > 1 ? stickers.filter((sticker) => selectedIds.includes(sticker.id)) : [],
    [stickers, selectedIds],
  );
  const working = useRef(false);
  // 拖出的表情经过本窗口时，webview 也会收到拖放事件。这些事件要在事件循环里排队，比拖拽结束的回调晚到，
  // 所以松手时还不知道落在哪：再等一会儿，等到这份副本的放下事件才说明拖回了本窗口。
  const outgoing = useRef<Outgoing | null>(null);
  const [busy, setBusy] = useState(false);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const feedbackTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [undo, setUndo] = useState<{ ids: string[]; label: string } | null>(null);
  const undoTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => () => { clearTimeout(feedbackTimer.current); clearTimeout(undoTimer.current); }, []);

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
      setError(reason instanceof Error ? reason.message : String(reason));
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

  const removeStickers = useCallback((targets: Sticker[], operation: "delete" | "restore" | "purge") => perform(async () => {
    if (!targets.length) return;
    const ids = targets.map((sticker) => sticker.id);
    const picked = new Set(ids);
    const first = stickers.findIndex((item) => picked.has(item.id));
    const last = stickers.length - 1 - [...stickers].reverse().findIndex((item) => picked.has(item.id));
    // 优先选中最后一张被删表情之后的第一张；后面没有了，再选第一张被删表情之前的最后一张。
    const nextId = first < 0 ? null
      : (stickers.slice(last + 1).find((item) => !picked.has(item.id)) ?? [...stickers.slice(0, first)].reverse().find((item) => !picked.has(item.id)))?.id ?? null;
    const focused = document.activeElement?.id.startsWith("sticker-") && picked.has(document.activeElement.id.slice(8));
    const { view, searchQuery } = useUiStore.getState();
    if (operation === "purge" && !await confirm(
      targets.length === 1
        ? "彻底删除后无法恢复。以后再导入同一张图，会作为一张新表情，分类、配文、收藏和使用次数都不会找回。"
        : `将彻底删除 ${targets.length} 张表情，无法恢复。以后再导入同一张图，会作为一张新表情，分类、配文、收藏和使用次数都不会找回。已设置同步时，其他设备上也会一起删除。`,
      { title: targets.length === 1 ? `彻底删除“${targets[0].name}”` : `彻底删除 ${targets.length} 张表情`, kind: "warning", okLabel: "彻底删除", cancelLabel: "取消" },
    )) return;
    if (operation === "purge") {
      if (ids.length === 1) await ipc.purgeSticker(ids[0]);
      else await ipc.purgeDeletedStickers(ids);
    } else {
      const changed = await ipc[operation === "delete" ? "deleteStickers" : "restoreStickers"](ids);
      if (!changed.length) throw new Error(operation === "delete" ? "这张表情已不在仓库里，或已被删除" : "这张表情已被恢复或彻底删除");
      if (operation === "delete") {
        clearTimeout(undoTimer.current);
        setUndo({ ids: changed, label: changed.length === 1 ? `已删除“${targets.find((sticker) => sticker.id === changed[0])?.name ?? ""}”` : `已删除 ${changed.length} 张表情` });
        undoTimer.current = setTimeout(() => setUndo(null), UNDO_MS);
      }
    }
    const current = useUiStore.getState();
    // 操作期间用户已切换列表或改了选择，就保留新的选择。
    if (current.selectedStickerIds.length !== ids.length || current.selectedStickerIds.some((id) => !picked.has(id)) || current.view !== view || current.searchQuery !== searchQuery) return;
    selectSticker(nextId);
    if (focused && nextId) requestAnimationFrame(() => document.getElementById(`sticker-${nextId}`)?.focus());
  }), [stickers, perform, selectSticker]);

  const deleteStickers = useCallback((targets: Sticker[]) => removeStickers(targets, "delete"), [removeStickers]);
  const deleteSticker = useCallback((sticker: Sticker) => removeStickers([sticker], "delete"), [removeStickers]);
  const restoreStickers = (targets: Sticker[]) => removeStickers(targets, "restore");
  const purgeStickers = (targets: Sticker[]) => removeStickers(targets, "purge");
  const restoreSticker = (sticker: Sticker) => restoreStickers([sticker]);
  const purgeSticker = (sticker: Sticker) => purgeStickers([sticker]);

  /** 撤销最近一次删除：把当时实际删掉的表情恢复，并重新选中。 */
  const undoDelete = () => perform(async () => {
    if (!undo) return;
    clearTimeout(undoTimer.current);
    setUndo(null);
    const restored = await ipc.restoreStickers(undo.ids);
    if (!restored.length) throw new Error("这些表情已被恢复或彻底删除");
    // 在最近删除里撤销时，恢复的表情不在当前列表里，不选中。
    if (useUiStore.getState().view !== "recentlyDeleted") setSelection(restored, restored[0]);
  });

  /** 右键点中的表情在多选里，操作对象是整个多选；否则只是它自己。 */
  const targetsFor = (sticker: Sticker) =>
    selectedStickers.some((item) => item.id === sticker.id) ? selectedStickers : [sticker];

  const purgeDeletedStickers = (ids: string[]) => perform(async () => {
    if (!ids.length || !await confirm(
      `将彻底删除 ${ids.length} 张表情，无法恢复。以后再导入同一张图，会作为一张新表情，分类、配文、收藏和使用次数都不会找回。已设置同步时，其他设备上也会一起删除。`,
      { title: "清空最近删除", kind: "warning", okLabel: "全部彻底删除", cancelLabel: "取消" },
    )) return;
    await ipc.purgeDeletedStickers(ids);
    const current = useUiStore.getState();
    if (current.view === "recentlyDeleted" && !current.searchQuery) {
      selectSticker(null);
      // 按钮会随空列表卸载；同步和刷新期间标题始终存在，不依赖后台 rAF。
      document.getElementById("sticker-grid-title")?.focus();
    }
  });

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const target = event.target;
      const targets = selectedStickers.length ? selectedStickers : selected ? [selected] : [];
      if (!targets.length || event.defaultPrevented || event.isComposing || event.keyCode === 229 || event.altKey) return;
      if (target instanceof HTMLElement && (target.closest("input, textarea, select, [role='textbox'], dialog, [role='dialog'], [role='listbox']") || target.isContentEditable)) return;
      if (event.key === "Escape" && selectedStickers.length) {
        event.preventDefault();
        selectSticker(selectedId);
        return;
      }
      if (targets.some((sticker) => sticker.deletedAt !== null)) return;
      const modifier = platform === "macos" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
      const copying = targets.length === 1 && modifier && event.key.toLowerCase() === "c";
      const deleting = !event.shiftKey && (platform === "macos"
        ? modifier && event.key === "Backspace"
        : event.key === "Delete" && !event.ctrlKey && !event.metaKey);
      if (!copying && !deleting) return;
      event.preventDefault();
      if (!event.repeat) void (deleting ? deleteStickers(targets) : copy(targets[0], event.shiftKey));
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selected, selectedStickers, selectedId, selectSticker, copy, deleteStickers]);

  function drag(sticker: Sticker, isPressed: () => boolean) {
    if (sticker.deletedAt !== null) return Promise.resolve();
    // 拖的是多选里的一张，保留多选；拖出的始终只有这一张。
    if (!useUiStore.getState().selectedStickerIds.includes(sticker.id)) selectSticker(sticker.id);
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

  function dismissUndo() {
    clearTimeout(undoTimer.current);
    setUndo(null);
  }

  function toggleFavorite(sticker: Sticker) {
    return perform(async () => { await ipc.toggleStickerFavorite(sticker.id); });
  }

  function reveal(sticker: Sticker) {
    return perform(() => ipc.revealSticker(sticker.id), false);
  }

  return { copy, drag, toggleFavorite, reveal, deleteSticker, deleteStickers, restoreSticker, restoreStickers, purgeSticker, purgeStickers, purgeDeletedStickers, selectedStickers, targetsFor, undo, undoDelete, dismissUndo, busy, claimDragDrop, copiedId, error };
}

/** 拖放事件里的路径写法可能和导出时不同（比如 /var 与 /private/var），只比较 export-xxx/文件名 这两段。 */
function sameExport(path: string, exported: string) {
  const tail = (value: string) => value.normalize("NFC").split(/[\\/]/).slice(-2).join("/");
  return tail(path) === tail(exported);
}
