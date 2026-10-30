import { DetailPanel } from "./DetailPanel";
import { Sidebar } from "./Sidebar";
import { StickerGrid } from "./StickerGrid";
import { TitleBar } from "./TitleBar";
import { useImport } from "./useImport";
import { useStickerActions } from "./useStickerActions";

export function App() {
  const actions = useStickerActions();
  const { choose, busy, dragging, message } = useImport(actions.draggingOut, actions.onInternalDrop);

  return (
    <div className={`flex h-full flex-col ${dragging ? "outline-2 -outline-offset-2 outline-accent" : ""}`}>
      <TitleBar busy={busy} onChoose={choose} />
      <div role="status" aria-atomic="true" className="max-h-24 shrink-0 overflow-y-auto whitespace-pre-line text-ink-secondary">
        {message && <p className="px-6 py-2">{message}</p>}
      </div>
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <StickerGrid actions={actions} />
        <DetailPanel actions={actions} />
      </div>
    </div>
  );
}
