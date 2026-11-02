import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { ipc, type Sticker } from "../../lib/ipc";
import { useLibraryChangedEvents, useStickerSearch, useStickers } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";
import { DetailPanel } from "./DetailPanel";
import { Sidebar } from "./Sidebar";
import { StickerGrid } from "./StickerGrid";
import { filterStickers, sortStickers } from "./stickerList";
import { TitleBar } from "./TitleBar";
import { useImport } from "./useImport";
import { PreferencesDialog } from "./PreferencesDialog";
import { usePreferences } from "./usePreferences";
import { useStickerActions } from "./useStickerActions";

// 列表还没读到时用同一个空数组，避免每次渲染都换引用、重新绑定快捷键。
const NO_STICKERS: Sticker[] = [];

export function App() {
  const preferences = usePreferences();
  const view = useUiStore((s) => s.view);
  const searchQuery = useUiStore((s) => s.searchQuery);
  const typeFilter = useUiStore((s) => s.typeFilter);
  const sortOrder = useUiStore((s) => s.sortOrder);
  const listing = useStickers(view);
  const search = useStickerSearch(searchQuery);
  const visible = searchQuery ? search : listing;
  const source = visible.data ?? NO_STICKERS;
  const sortable = !searchQuery && view !== "recent" && view !== "recentlyDeleted";
  const stickers = useMemo(() => {
    const filtered = filterStickers(source, typeFilter);
    return sortable ? sortStickers(filtered, sortOrder) : filtered;
  }, [source, typeFilter, sortOrder, sortable]);
  const actions = useStickerActions(stickers);
  const { choose, busy, dragging, message } = useImport(actions.claimDragDrop);
  const eventError = useLibraryChangedEvents();
  const shortcut = useQuery({ queryKey: ["global-shortcut-error"], queryFn: ipc.globalShortcutError });
  const status = [message, eventError, preferences.error, shortcut.data, shortcut.error && String(shortcut.error)].filter(Boolean).join("\n");

  return (
    <div className={`flex h-full flex-col ${dragging ? "outline-2 -outline-offset-2 outline-accent" : ""}`}>
      {preferences.open && <PreferencesDialog onClose={() => preferences.setOpen(false)} />}
      <TitleBar busy={busy} onChoose={choose} />
      <div role="status" aria-atomic="true" className="max-h-24 shrink-0 overflow-y-auto whitespace-pre-line text-ink-secondary">
        {status && <p className="px-6 py-2">{status}</p>}
      </div>
      <div className="flex min-h-0 flex-1">
        <Sidebar onPreferences={() => preferences.setOpen(true)} />
        <StickerGrid actions={actions} listing={visible} stickers={stickers} sortable={sortable} />
        <DetailPanel actions={actions} />
      </div>
    </div>
  );
}
