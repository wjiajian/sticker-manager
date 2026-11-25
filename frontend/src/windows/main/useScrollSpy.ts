import { useCallback, useLayoutEffect, useRef, useState, type RefObject } from "react";
import type { SectionId } from "./preferencesSections";

export function useScrollSpy(scroller: RefObject<HTMLDivElement | null>, visibleKey: string, clearQuery: () => void) {
  const [active, setActive] = useState<SectionId | undefined>("appearance");
  const pinned = useRef<SectionId | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const deferred = useRef(0);
  const measure = useCallback(() => {
    const root = scroller.current;
    // Before showModal(), every heading has a zero rectangle. Keep the opening anchor.
    if (!root?.clientHeight) return;
    const sections = Array.from(root.querySelectorAll<HTMLElement>("[data-section]:not([hidden])"));
    const top = root.getBoundingClientRect().top;
    const target = sections.find(section => section.dataset.section === pinned.current);
    if (target && Math.abs(target.getBoundingClientRect().top - top - 24) > 2) return;
    pinned.current = null;
    const bottom = root.scrollHeight > root.clientHeight && root.scrollTop + root.clientHeight >= root.scrollHeight - 2;
    const section = bottom ? sections.at(-1) : sections.filter(section => section.getBoundingClientRect().top - top <= 64).at(-1) ?? sections[0];
    setActive(section?.dataset.section as SectionId | undefined);
  }, [scroller]);
  useLayoutEffect(() => {
    const root = scroller.current!;
    let frame = 0;
    const schedule = () => { if (!frame) frame = requestAnimationFrame(() => { frame = 0; measure(); }); };
    const unpin = () => { pinned.current = null; schedule(); };
    const observer = new ResizeObserver(schedule);
    observer.observe(root);
    root.querySelectorAll("[data-section]").forEach(section => observer.observe(section));
    root.addEventListener("scroll", schedule, { passive: true });
    for (const event of ["wheel", "pointerdown", "keydown"]) root.addEventListener(event, unpin, { passive: true });
    unpin();
    return () => {
      observer.disconnect();
      root.removeEventListener("scroll", schedule);
      for (const event of ["wheel", "pointerdown", "keydown"]) root.removeEventListener(event, unpin);
      cancelAnimationFrame(frame);
      cancelAnimationFrame(deferred.current);
      clearTimeout(timer.current);
    };
  }, [scroller, measure]);
  useLayoutEffect(() => { pinned.current = null; measure(); }, [visibleKey, measure]);

  const jumpTo = useCallback((id: SectionId, { focus, smooth }: { focus: boolean; smooth: boolean }) => {
    const root = scroller.current;
    const section = root?.querySelector<HTMLElement>(`[data-section="${id}"]`);
    if (!root || !section) return;
    cancelAnimationFrame(deferred.current);
    const jump = () => {
      pinned.current = id;
      setActive(id);
      if (focus) section.querySelector("h3")?.focus({ preventScroll: true });
      // WKWebView also scrolls the outer dialog with scrollIntoView, hiding its toolbar.
      const margin = parseFloat(getComputedStyle(section).scrollMarginTop) || 0;
      root.scrollTo({
        top: root.scrollTop + section.getBoundingClientRect().top - root.getBoundingClientRect().top - margin,
        behavior: smooth && !matchMedia("(prefers-reduced-motion: reduce)").matches ? "smooth" : "auto",
      });
      clearTimeout(timer.current);
      timer.current = setTimeout(() => { pinned.current = null; measure(); }, 1000);
    };
    if (section.hidden) { clearQuery(); deferred.current = requestAnimationFrame(jump); }
    else jump();
  }, [scroller, measure, clearQuery]);

  return { active, jumpTo };
}
