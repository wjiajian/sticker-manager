import { CloudSlashIcon, MagnifyingGlassIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { platform } from "../../lib/platform";
import { useUiStore } from "../../stores/ui";
import { ImportButton } from "./ImportButton";

export function TitleBar({ busy, onChoose }: { busy: boolean; onChoose: (directory: boolean) => void }) {
  const input = useRef<HTMLInputElement>(null);
  const searchText = useUiStore((s) => s.searchText);
  const setSearchText = useUiStore((s) => s.setSearchText);
  const setSearchQuery = useUiStore((s) => s.setSearchQuery);
  const requestResultFocus = useUiStore((s) => s.requestResultFocus);
  const [composing, setComposing] = useState(false);

  useEffect(() => {
    if (composing) return;
    const query = searchText.trim();
    if (!query) { setSearchQuery(""); return; }
    const timer = setTimeout(() => setSearchQuery(query), 150);
    return () => clearTimeout(timer);
  }, [searchText, composing, setSearchQuery]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const modifier = platform === "macos" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
      if (!modifier || event.altKey || event.shiftKey || event.key.toLowerCase() !== "k") return;
      event.preventDefault();
      input.current?.focus();
      input.current?.select();
    }
    // 捕获阶段处理，配文和标签编辑框有焦点时也能跳到搜索。
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);

  return (
    <header data-tauri-drag-region className="flex items-center gap-4 bg-chrome px-4 py-3">
      {/* macOS 的红绿灯按钮浮在这块空白上，位置在 src-tauri/src/lib.rs 里设置 */}
      {platform === "macos" && <div data-tauri-drag-region className="w-50 shrink-0 self-stretch" />}

      <label className="interactive mx-auto flex max-w-120 flex-1 items-center gap-2 rounded-control border border-line bg-surface px-3 py-2 focus-within:border-accent">
        <MagnifyingGlassIcon size={16} className="shrink-0 text-ink-secondary" />
        <input
          ref={input}
          aria-label="搜索表情"
          placeholder="搜索表情、标签或配文"
          value={searchText}
          onChange={(event) => setSearchText(event.target.value)}
          onCompositionStart={() => setComposing(true)}
          onCompositionEnd={(event) => { setSearchText(event.currentTarget.value); setComposing(false); }}
          onKeyDown={(event) => {
            if (composing || event.nativeEvent.isComposing || event.keyCode === 229) return;
            if (event.key === "Escape") {
              event.preventDefault();
              if (searchText) { setSearchText(""); setSearchQuery(""); }
              else event.currentTarget.blur();
            } else if (searchText.trim() && (event.key === "ArrowDown" || event.key === "Enter")) {
              event.preventDefault();
              // 不等防抖：立即按现在的文字搜索，结果回来后由网格选中第一张，免得选到上一次的结果。
              setSearchQuery(searchText.trim());
              requestResultFocus(true);
            }
          }}
          className="min-w-0 flex-1 bg-transparent outline-none"
        />
        <kbd className="font-sans text-xs text-ink-secondary">{platform === "macos" ? "⌘K" : "Ctrl+K"}</kbd>
      </label>

      <div className="flex items-center gap-2">
        <Button>
          <CloudSlashIcon size={16} className="text-ink-secondary" />
          未设置同步
        </Button>
        <ImportButton disabled={busy} onChoose={onChoose} />
      </div>
    </header>
  );
}
