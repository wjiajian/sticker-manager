import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import { SyncDetails } from "./SyncDetails";
import { ImportBar } from "./ImportBar";
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
import { useSyncStatus } from "./useSyncStatus";

// 列表还没读到时用同一个空数组，避免每次渲染都换引用、重新绑定快捷键。
const NO_STICKERS: Sticker[] = [];

export function App() {
  const preferences = usePreferences();
  const sync = useSyncStatus();
  const [anchor, setAnchor] = useState<{ button: HTMLButtonElement; above: boolean } | null>(null);
  const anchorRef = useRef(anchor); anchorRef.current = anchor;
  const closeSync = useCallback((focus = false) => {
    if (focus) anchorRef.current?.button.focus();
    setAnchor(null);
  }, []);
  function openSync(button: HTMLButtonElement, above = false) {
    if (!sync.data?.target) { preferences.show(true); return; }
    setAnchor((old) => old?.button === button ? null : { button, above });
  }
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const show = () => { const button = document.getElementById("title-sync"); if (!disposed && button instanceof HTMLButtonElement) setAnchor({ button, above: false }); };
    const listening = listen("open-sync-details", () => { show(); void ipc.takeSyncDetailsRequest(); });
    void listening.then(() => ipc.takeSyncDetailsRequest()).then((pending) => { if (pending) show(); }).catch(() => {});
    return () => { disposed = true; void listening.then((unlisten) => unlisten(), () => {}); };
  }, []);
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
  const importing = useImport(actions.claimDragDrop);
  const { choose, busy, dragging, message } = importing;
  const eventError = useLibraryChangedEvents();
  const shortcut = useQuery({ queryKey: ["global-shortcut-error"], queryFn: ipc.globalShortcutError });
  const status = [message, eventError, preferences.error, sync.error && String(sync.error), shortcut.data, shortcut.error && String(shortcut.error)].filter(Boolean).join("\n");

  return (
    <div className={`flex h-full flex-col ${dragging ? "outline-2 -outline-offset-2 outline-accent" : ""}`}>
      {preferences.open && <PreferencesDialog onClose={() => preferences.setOpen(false)} sync={sync.data} focusSync={preferences.focusSync} />}
      <TitleBar busy={busy} onChoose={choose} sync={sync.data} onSync={(button) => openSync(button)} />
      {status && <div role="status" aria-atomic="true" className="max-h-24 shrink-0 overflow-y-auto whitespace-pre-line text-ink-secondary">
        {status && <p className="px-6 py-2">{status}</p>}
      </div>}
      <ImportBar key={importing.status?.batch ?? 0} status={importing.status} error={importing.error} clearError={importing.clearError} />
      {anchor && sync.data?.target && !preferences.open && <SyncDetails status={sync.data} anchor={anchor.button} above={anchor.above} onClose={closeSync} onSettings={() => { closeSync(); preferences.show(true); }} />}
      <div className="flex min-h-0 flex-1">
        <Sidebar onPreferences={() => preferences.show()} sync={sync.data} onSync={(button) => openSync(button, true)} />
        <StickerGrid actions={actions} listing={visible} stickers={stickers} sortable={sortable} />
        <DetailPanel actions={actions} sync={sync.data} />
      </div>
    </div>
  );
}
