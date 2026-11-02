import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, type PointerEvent } from "react";
import { ipc, type PreviewSticker } from "../../lib/ipc";

type Candidate = { key: string; element: HTMLElement };

export function usePreview(items: Map<string, PreviewSticker>, busy: boolean, searchText: string, onError: (message: string) => void) {
  const data = useRef(items); data.current = items;
  const blocked = useRef(busy); blocked.current = busy;
  const report = useRef(onError); report.current = onError;
  const hover = useRef<Candidate | null>(null);
  const visible = useRef(false);
  const requested = useRef<string | null>(null);
  const lastHidden = useRef(-Infinity);
  const pointer = useRef<{ x: number; y: number } | null>(null);
  const lastMove = useRef<{ x: number; y: number } | null>(null);
  const showTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const sent = useRef<PreviewSticker | null>(null);

  const clearTimers = useCallback(() => { clearTimeout(showTimer.current); clearTimeout(hideTimer.current); }, []);
  const clear = useCallback(() => {
    clearTimers(); hover.current = null; requested.current = null; sent.current = null;
    visible.current = false; lastHidden.current = -Infinity;
    pointer.current = null;
  }, [clearTimers]);
  const hide = useCallback((warm: boolean) => {
    if (warm && visible.current) lastHidden.current = performance.now();
    if (!warm) lastHidden.current = -Infinity;
    visible.current = false; requested.current = null; sent.current = null;
    void ipc.hidePreview().catch((error: unknown) => report.current(String(error)));
  }, []);
  const reset = useCallback(() => { clear(); hide(false); }, [clear, hide]);

  const show = useCallback(() => {
    const candidate = hover.current;
    const sticker = candidate && data.current.get(candidate.key);
    if (blocked.current || !candidate?.element.isConnected || !sticker) return;
    const rect = candidate.element.getBoundingClientRect();
    requested.current = candidate.key; sent.current = sticker;
    void ipc.showPreview(sticker, { top: rect.top, height: rect.height }).catch((error: unknown) => report.current(String(error)));
  }, []);

  const enter = useCallback((key: string, element: HTMLElement) => {
    clearTimers(); hover.current = { key, element };
    if (requested.current === key) return;
    if (visible.current || performance.now() - lastHidden.current < 300) show();
    else showTimer.current = setTimeout(show, 400);
  }, [clearTimers, show]);

  useEffect(() => {
    if (!isTauri()) return;
    const listeners = [
      listen<boolean>("preview-visibility", ({ payload }) => {
        if (payload && requested.current) visible.current = true;
        if (!payload) visible.current = false;
      }),
      listen("preview-reset", clear),
    ];
    void Promise.all(listeners).catch((error: unknown) => report.current(String(error)));
    return () => { clear(); void ipc.hidePreview(); for (const pending of listeners) void pending.then((unlisten) => unlisten(), () => {}); };
  }, [clear]);

  useEffect(() => { reset(); }, [searchText, reset]);
  useEffect(() => { if (busy) reset(); }, [busy, reset]);
  useEffect(() => {
    const key = requested.current ?? hover.current?.key;
    if (!key) return;
    const next = items.get(key);
    if (!next) { reset(); return; }
    const old = sent.current;
    if (old && (old.name !== next.name || old.usageCount !== next.usageCount || old.thumbnailPath !== next.thumbnailPath
      || old.objectPath !== next.objectPath || old.tags.length !== next.tags.length || old.tags.some((tag, index) => tag !== next.tags[index]))) show();
  }, [items, reset, show]);

  function move(key: string, event: PointerEvent<HTMLElement>) {
    if (event.pointerType !== "mouse") return;
    const previous = lastMove.current;
    lastMove.current = { x: event.clientX, y: event.clientY };
    pointer.current = { x: event.clientX, y: event.clientY };
    if (event.buttons || blocked.current) return;
    if (hover.current?.key !== key) { enter(key, event.currentTarget); return; }
    if (!requested.current && (previous?.x !== event.clientX || previous?.y !== event.clientY)) {
      clearTimeout(showTimer.current); showTimer.current = setTimeout(show, 400);
    }
  }

  return {
    reset,
    onPointerMove(event: PointerEvent<HTMLElement>) {
      if (event.pointerType === "mouse" && !event.buttons) pointer.current = { x: event.clientX, y: event.clientY };
    },
    onPointerLeave() { pointer.current = null; },
    onScroll() {
      const point = pointer.current;
      reset();
      pointer.current = point;
      // 停止滚动后按首次悬停处理；格子可能已经滚到了静止鼠标的下面。
      showTimer.current = setTimeout(() => {
        const current = pointer.current;
        if (!current || blocked.current) return;
        const element = document.elementFromPoint(current.x, current.y)?.closest<HTMLElement>("[data-preview-key]");
        const key = element?.dataset.previewKey;
        if (element && key && data.current.has(key)) { hover.current = { key, element }; show(); }
      }, 400);
    },
    tile(key: string) {
      return {
        "data-preview-key": key,
        onPointerEnter(event: PointerEvent<HTMLElement>) { move(key, event); },
        onPointerMove(event: PointerEvent<HTMLElement>) { move(key, event); },
        onPointerLeave() {
          if (hover.current?.key !== key) return;
          hover.current = null; clearTimeout(showTimer.current);
          hideTimer.current = setTimeout(() => hide(true), 150);
        },
      };
    },
  };
}
