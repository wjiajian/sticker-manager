import type { Sticker } from "../../lib/ipc";
import { useLibraryChangedEvents, useStickerSearch, useStickers } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";
import { DetailPanel } from "./DetailPanel";
import { Sidebar } from "./Sidebar";
import { StickerGrid } from "./StickerGrid";
import { TitleBar } from "./TitleBar";
import { useImport } from "./useImport";
import { useStickerActions } from "./useStickerActions";

// 列表还没读到时用同一个空数组，避免每次渲染都换引用、重新绑定快捷键。
const NO_STICKERS: Sticker[] = [];

export function App() {
  const view = useUiStore((s) => s.view);
  const searchQuery = useUiStore((s) => s.searchQuery);
  const listing = useStickers(view);
  const search = useStickerSearch(searchQuery);
  const visible = searchQuery ? search : listing;
  const actions = useStickerActions(visible.data ?? NO_STICKERS);
  const { choose, busy, dragging, message } = useImport(actions.claimDragDrop);
  const eventError = useLibraryChangedEvents();
  const status = [message, eventError].filter(Boolean).join("\n");

  return (
    <div className={`flex h-full flex-col ${dragging ? "outline-2 -outline-offset-2 outline-accent" : ""}`}>
      <TitleBar busy={busy} onChoose={choose} />
      <div role="status" aria-atomic="true" className="max-h-24 shrink-0 overflow-y-auto whitespace-pre-line text-ink-secondary">
        {status && <p className="px-6 py-2">{status}</p>}
      </div>
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <StickerGrid actions={actions} listing={visible} />
        <DetailPanel actions={actions} />
      </div>
    </div>
  );
}
