import { CaretDownIcon, type Icon } from "@phosphor-icons/react";
import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { CategoryDot } from "../../components/CategoryDot";
import type { StickerPageFilter } from "../../lib/ipc";
import { compactNavigation, selectedNavigation } from "../main/libraryViews";

/** 列表放不下时，和面板边缘保持的距离。 */
const PANEL_MARGIN = 8;

export interface CategoryFilterOption {
  filter: StickerPageFilter;
  name: string;
  count?: number;
  icon?: Icon;
  color?: string;
}

function OptionMark({ option }: { option: CategoryFilterOption }) {
  return <span className="flex size-4 shrink-0 items-center justify-center">
    {option.icon ? <option.icon size={16} aria-hidden /> : option.color && <CategoryDot color={option.color} />}
  </span>;
}

/**
 * 不用原生下拉框：面板失去焦点就会收起，弹出列表必须留在面板窗口里。
 * 列表像 macOS 的弹出按钮一样盖在按钮上：选中项压在按钮原位，其他项向上下展开。
 * 打开时焦点移到列表上，方向键和回车由列表处理；关闭后由 onClose 把焦点还给搜索框。
 * 列表的行和侧栏同款，底色也用侧栏的 chrome，选中态才看得出来。
 */
export function CategoryFilter({ headingId, options, selected, onSelect, onClose }: {
  headingId: string;
  options: CategoryFilterOption[];
  selected: CategoryFilterOption;
  onSelect: (filter: StickerPageFilter) => void;
  onClose: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const listId = useId();
  const activeIndex = Math.min(active, options.length - 1);
  const activeId = `${listId}-${activeIndex}`;

  useLayoutEffect(() => {
    const element = list.current;
    const button = trigger.current;
    const row = element?.querySelector<HTMLElement>('[aria-selected="true"]');
    const panel = element?.closest("main")?.getBoundingClientRect();
    if (!open || !element || !button || !row || !panel) return;
    const rowTop = button.getBoundingClientRect().top + (button.offsetHeight - row.offsetHeight) / 2;
    // 选中项始终留在按钮原位：上方放不下时靠滚动，下方放不下时把列表截短。
    const top = Math.max(rowTop - row.offsetTop, panel.top + PANEL_MARGIN);
    const scrollTop = top + row.offsetTop - rowTop;
    element.style.top = `${top}px`;
    element.style.height = `${Math.min(element.scrollHeight - scrollTop, panel.bottom - PANEL_MARGIN - top)}px`;
    element.scrollTop = scrollTop;
  }, [open]);

  useEffect(() => {
    if (open) list.current?.focus({ preventScroll: true });
  }, [open]);

  useEffect(() => {
    if (open) document.getElementById(activeId)?.scrollIntoView({ block: "nearest" });
  }, [open, activeId]);

  useEffect(() => {
    if (!open) return;
    // 固定的面板失去焦点时不会收起，列表要和原生弹出菜单一样自己收起。
    window.addEventListener("blur", close);
    return () => window.removeEventListener("blur", close);
  }, [open]);

  function show() {
    setActive(options.indexOf(selected));
    setOpen(true);
  }

  function close() {
    setOpen(false);
    onClose();
  }

  function choose(option: CategoryFilterOption) {
    onSelect(option.filter);
    close();
  }

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    // 面板外层的 Esc 会收起整个面板，这里只收起列表。
    event.stopPropagation();
    if (event.key === "Escape" || event.key === "Tab") { event.preventDefault(); close(); }
    else if (event.key === "Enter" || event.key === " ") { event.preventDefault(); choose(options[activeIndex]); }
    else if (event.key === "ArrowDown") { event.preventDefault(); setActive(Math.min(options.length - 1, activeIndex + 1)); }
    else if (event.key === "ArrowUp") { event.preventDefault(); setActive(Math.max(0, activeIndex - 1)); }
  }

  return <>
    <h2 id={headingId} className="min-w-0">
      <button ref={trigger} type="button" aria-haspopup="listbox" aria-expanded={open} aria-controls={open ? listId : undefined}
        className="interactive flex max-w-full items-center gap-2 rounded-sm hover:opacity-82 active:scale-98"
        onMouseDown={(event) => event.preventDefault()} onClick={() => { if (open) close(); else show(); }}>
        <OptionMark option={selected} />
        <span className="flex min-w-0 items-center gap-1">
          <span className="truncate">{selected.name}</span>
          <CaretDownIcon size={12} className="shrink-0 text-ink-secondary" aria-hidden />
        </span>
      </button>
    </h2>
    {open && <>
      {/* 点列表外面只收起列表，不复制底下的表情。 */}
      <div className="fixed inset-0 z-10" aria-hidden onMouseDown={(event) => { event.preventDefault(); close(); }} />
      {/* 左边距让行内容和按钮内容对齐，宽度让行对齐网格第一、二列。 */}
      <div ref={list} id={listId} role="listbox" aria-label="按分类筛选" aria-activedescendant={activeId} tabIndex={-1}
        className="fixed left-2 z-20 flex w-43 flex-col gap-0.5 overflow-y-auto overscroll-contain rounded-card bg-chrome p-1 text-ink shadow-(--shadow-preview) outline-none transition-opacity duration-300 ease-standard starting:opacity-0"
        onKeyDown={onKeyDown}>
        {options.map((option, index) => <button key={index} id={`${listId}-${index}`} type="button" role="option" aria-selected={option === selected} tabIndex={-1}
          title={option.name} data-active={index === activeIndex} onMouseDown={(event) => event.preventDefault()} onMouseMove={() => setActive(index)} onClick={() => choose(option)}
          className={`${compactNavigation} shrink-0 data-[active=true]:bg-hover ${option === selected ? selectedNavigation : option.icon ? "font-medium" : ""}`}>
          <OptionMark option={option} />
          <span className="min-w-0 flex-1 truncate">{option.name}</span>
          {option.count !== undefined && <span className="shrink-0 font-normal text-ink-secondary">{option.count}</span>}
        </button>)}
      </div>
    </>}
  </>;
}
