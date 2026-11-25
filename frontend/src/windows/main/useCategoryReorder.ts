import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type PointerEvent } from "react";
import { ipc, type CategorySummary } from "../../lib/ipc";
import { useLibraryMutation } from "../../lib/queries";
import { sidebarScrollDirection } from "./stickerDrop";

const categoriesKey = ["library", "categories"];

interface Gesture {
  row: HTMLElement;
  pointer: number;
  x: number;
  startY: number;
  y: number;
  active: boolean;
  before: string | null;
}

export function useCategoryReorder(list: CategorySummary[] | undefined, disabled: boolean) {
  const client = useQueryClient();
  const mutation = useLibraryMutation(({ id, before }: { id: string; before: string | null }) => ipc.moveCategory(id, before));
  const gesture = useRef<Gesture | null>(null);
  const suppress = useRef(false);
  const locked = useRef(false);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [indicatorTop, setIndicatorTop] = useState<number | null>(null);
  const [announcement, announce] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function move(id: string, before: string | null) {
    const original = list?.findIndex((category) => category.id === id) ?? -1;
    if (disabled || locked.current || original < 0 || before === id || before === (list?.[original + 1]?.id ?? null)) return;
    locked.current = true;
    const row = document.getElementById(`sidebar-category-${id}`);
    const focused = row && document.activeElement === row;
    try {
      setError(null);
      await client.cancelQueries({ queryKey: categoriesKey });
      const current = client.getQueryData<CategorySummary[]>(categoriesKey) ?? [];
      const category = current.find((entry) => entry.id === id);
      const order = current.filter((entry) => entry.id !== id);
      const index = before === null ? order.length : order.findIndex((entry) => entry.id === before);
      if (!category || index < 0 || current[index]?.id === id) return;
      order.splice(index, 0, category);
      client.setQueryData(categoriesKey, order.map((entry, position) => ({ ...entry, position })));
      if (focused) requestAnimationFrame(() => {
        if (!document.activeElement || document.activeElement === document.body) {
          document.getElementById(`sidebar-category-${id}`)?.focus();
        }
      });
      await mutation.mutateAsync({ id, before });
      announce(`“${category.name}”已移到第 ${index + 1} 位`);
    } catch (reason) {
      setError(String(reason));
    } finally {
      locked.current = false;
    }
  }

  async function moveBy(id: string, delta: -1 | 1) {
    const index = list?.findIndex((row) => row.id === id) ?? -1;
    if (!list || index < 0 || !list[index + delta]) return;
    await move(id, delta < 0 ? list[index - 1].id : list[index + 2]?.id ?? null);
  }

  function finish() {
    const g = gesture.current;
    gesture.current = null;
    if (g?.row.hasPointerCapture(g.pointer)) g.row.releasePointerCapture(g.pointer);
    setDraggingId(null);
    setIndicatorTop(null);
  }
  // 取消的拖动后面不会有要拦的点击，复位，免得吞掉下一次正常点击。
  function cancelGesture() {
    suppress.current = false;
    finish();
  }

  function measure(g: Gesture) {
    const section = g.row.closest("section");
    if (!section) return;
    const rows = Array.from(section.querySelectorAll<HTMLElement>("[data-category-row]"));
    const next = rows.find((row) => {
      const rect = row.getBoundingClientRect();
      return g.y < rect.top + rect.height / 2;
    });
    g.before = next?.dataset.categoryRow ?? null;
    const successor = rows[rows.indexOf(g.row) + 1];
    const edge = next?.getBoundingClientRect().top ?? rows.at(-1)?.getBoundingClientRect().bottom;
    setIndicatorTop(next === g.row || next === successor || edge === undefined ? null : edge - section.getBoundingClientRect().top);
  }

  useEffect(() => {
    if (disabled || (draggingId && !list?.some((row) => row.id === draggingId))) finish();
  }, [disabled, draggingId, list]);

  useEffect(() => {
    if (!draggingId) return;
    document.body.classList.add("cursor-grabbing");
    function cancel(event: globalThis.KeyboardEvent) {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      cancelGesture();
    }
    window.addEventListener("keydown", cancel, true);
    let frame = 0;
    function scroll() {
      const g = gesture.current;
      if (!g?.active) return;
      const container = g.row.closest("div.overflow-y-auto");
      if (container) {
        const direction = sidebarScrollDirection(container.getBoundingClientRect(), g.y);
        if (direction) {
          container.scrollBy(0, direction * 8);
          measure(g);
        }
      }
      frame = requestAnimationFrame(scroll);
    }
    frame = requestAnimationFrame(scroll);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("keydown", cancel, true);
      document.body.classList.remove("cursor-grabbing");
    };
  }, [draggingId]);

  function rowProps(id: string) {
    return {
      "data-category-row": id,
      onPointerDown(event: PointerEvent<HTMLElement>) {
        if (event.button !== 0 || !event.isPrimary) return;
        suppress.current = false;
        if (disabled || locked.current || (list?.length ?? 0) < 2) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        gesture.current = { row: event.currentTarget, pointer: event.pointerId, x: event.clientX, startY: event.clientY, y: event.clientY, active: false, before: id };
      },
      onPointerMove(event: PointerEvent<HTMLElement>) {
        const g = gesture.current;
        if (!g || g.pointer !== event.pointerId) return;
        if (!(event.buttons & 1)) { cancelGesture(); return; }
        g.y = event.clientY;
        if (!g.active) {
          if (Math.hypot(event.clientX - g.x, g.y - g.startY) <= 5) return;
          g.active = true;
          suppress.current = true;
          setDraggingId(id);
        }
        measure(g);
      },
      onPointerUp(event: PointerEvent<HTMLElement>) {
        const g = gesture.current;
        if (!g || g.pointer !== event.pointerId) return;
        if (g.active) {
          g.y = event.clientY;
          measure(g);
          if (g.before !== id) void move(id, g.before);
          // 保留拦截，挡住紧跟的合成点击；在行外松手不会有点击，下个宏任务兜底复位。
          setTimeout(() => { suppress.current = false; }, 0);
        }
        finish();
      },
      onPointerCancel(event: PointerEvent<HTMLElement>) { if (gesture.current?.pointer === event.pointerId) cancelGesture(); },
      // 正常松手时 finish 先清掉 gesture 再释放捕获，这里不会误清拦截。
      onLostPointerCapture(event: PointerEvent<HTMLElement>) { if (gesture.current?.pointer === event.pointerId) cancelGesture(); },
      onClickCapture(event: MouseEvent<HTMLElement>) {
        if (!suppress.current || !event.detail) return;
        event.preventDefault();
        event.stopPropagation();
        suppress.current = false;
      },
      onKeyDown(event: KeyboardEvent<HTMLElement>) {
        if (!event.altKey || event.nativeEvent.isComposing || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return;
        event.preventDefault();
        void moveBy(id, event.key === "ArrowUp" ? -1 : 1);
      },
    };
  }
  return { rowProps, indicatorTop, draggingId, moveBy, error, pending: mutation.isPending, announcement };
}
