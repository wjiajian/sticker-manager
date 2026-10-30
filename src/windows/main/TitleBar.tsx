import { CloudSlashIcon, MagnifyingGlassIcon, PlusIcon } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { platform } from "../../lib/platform";

export function TitleBar() {
  return (
    <header data-tauri-drag-region className="flex items-center gap-4 bg-chrome px-4 py-3">
      {/* macOS 的红绿灯按钮浮在这块空白上，位置在 src-tauri/src/lib.rs 里设置 */}
      {platform === "macos" && <div data-tauri-drag-region className="w-50 shrink-0 self-stretch" />}

      <label className="interactive mx-auto flex max-w-120 flex-1 items-center gap-2 rounded-control border border-line bg-surface px-3 py-2 focus-within:border-accent">
        <MagnifyingGlassIcon size={16} className="shrink-0 text-ink-secondary" />
        <input
          aria-label="搜索表情"
          placeholder="搜索表情、标签或配文"
          className="min-w-0 flex-1 bg-transparent outline-none"
        />
        <kbd className="font-sans text-xs text-ink-secondary">{platform === "macos" ? "⌘K" : "Ctrl+K"}</kbd>
      </label>

      <div className="flex items-center gap-2">
        <Button>
          <CloudSlashIcon size={16} className="text-ink-secondary" />
          未设置同步
        </Button>
        <Button variant="primary">
          <PlusIcon size={16} />
          导入表情
        </Button>
      </div>
    </header>
  );
}
