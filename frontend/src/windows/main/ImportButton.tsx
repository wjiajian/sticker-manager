import { CaretDownIcon, FolderOpenIcon, PlusIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/Button";

interface ImportButtonProps {
  disabled: boolean;
  onChoose: (directory: boolean) => void;
}

export function ImportButton({ disabled, onChoose }: ImportButtonProps) {
  const [open, setOpen] = useState(false);
  const container = useRef<HTMLDivElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const folder = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    folder.current?.focus();
    const dismiss = (event: PointerEvent) => {
      if (!container.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", dismiss);
    return () => document.removeEventListener("pointerdown", dismiss);
  }, [open]);

  return (
    <div
      ref={container}
      className="relative flex"
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape" && open) {
          event.preventDefault();
          setOpen(false);
          toggle.current?.focus();
        }
      }}
    >
      <Button
        variant="primary"
        disabled={disabled}
        onClick={() => onChoose(false)}
        className="rounded-r-none disabled:pointer-events-none disabled:opacity-50"
      >
        <PlusIcon size={16} />
        导入表情
      </Button>
      <button
        ref={toggle}
        type="button"
        aria-label="更多导入方式"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? "import-menu" : undefined}
        disabled={disabled}
        onClick={() => setOpen(!open)}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            setOpen(true);
          }
        }}
        className="interactive flex items-center rounded-r-control border-l border-surface/20 bg-ink px-2 text-surface hover:opacity-82 active:scale-98 disabled:pointer-events-none disabled:opacity-50"
      >
        <CaretDownIcon size={16} />
      </button>
      {open && (
        <div id="import-menu" role="menu" aria-label="导入方式" className="absolute top-full right-0 z-10 mt-2 min-w-40 rounded-control border border-line bg-surface p-1">
          <button
            ref={folder}
            type="button"
            role="menuitem"
            disabled={disabled}
            // WebKit 点击按钮会先失焦，避免 onBlur 在 click 前卸载菜单。
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => {
              setOpen(false);
              toggle.current?.focus();
              onChoose(true);
            }}
            onKeyDown={(event) => {
              if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) event.preventDefault();
            }}
            className="interactive flex w-full items-center gap-2 rounded-control px-3 py-2 text-left hover:bg-hover active:scale-98 disabled:opacity-50"
          >
            <FolderOpenIcon size={16} />
            导入文件夹
          </button>
        </div>
      )}
    </div>
  );
}
