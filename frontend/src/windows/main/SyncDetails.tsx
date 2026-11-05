import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useQueryClient } from "@tanstack/react-query";
import { Button } from "../../components/Button";
import { ipc, type SyncStatus } from "../../lib/ipc";
import { SyncDevices } from "./SyncDevices";
import { SyncIcon } from "./SyncIcon";
import { progressLabel, syncStateLabel, syncTime, waitingNote } from "./useSyncStatus";

export function SyncDetails({ status, anchor, above, onClose, onSettings }: { status: SyncStatus; anchor: HTMLButtonElement; above: boolean; onClose: (focus?: boolean) => void; onSettings: () => void }) {
  const panel = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: 0, top: 0 });
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const client = useQueryClient();
  useLayoutEffect(() => {
    function place() {
      const rect = anchor.getBoundingClientRect();
      const height = panel.current?.offsetHeight ?? 0;
      setPosition({ left: Math.max(8, Math.min(window.innerWidth - 328, above ? rect.left : rect.right - 320)), top: Math.max(8, Math.min(window.innerHeight - height - 8, above ? rect.top - height - 8 : rect.bottom + 8)) });
    }
    place();
    const observer = new ResizeObserver(place);
    if (panel.current) observer.observe(panel.current);
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => { observer.disconnect(); window.removeEventListener("resize", place); window.removeEventListener("scroll", place, true); };
  }, [anchor, above]);
  useEffect(() => {
    panel.current?.focus();
    function outside(event: PointerEvent) {
      if (event.target instanceof Node && !panel.current?.contains(event.target) && !anchor.contains(event.target)) onClose();
    }
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [anchor, onClose]);
  async function run(action: () => Promise<unknown>) {
    setBusy(true); setError(null);
    try { await action(); await client.invalidateQueries({ queryKey: ["sync"] }); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }
  return createPortal(<div id="sync-details" ref={panel} role="dialog" aria-label="同步详情" tabIndex={-1}
    style={position} className="fixed z-40 flex max-h-[calc(100vh-16px)] w-80 flex-col gap-4 overflow-y-auto rounded-card border border-line bg-surface p-4"
    onKeyDown={(event) => { if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); onClose(true); } }}>
    <h2 className="flex items-center gap-2 font-semibold"><SyncIcon status={status} /><span>{syncStateLabel(status)}</span></h2>
    <div className="text-xs"><p className="font-semibold">{status.target?.name}</p><p className="mt-1 select-text break-all text-ink-secondary">{status.target?.address}</p></div>
    {status.progress && <div className="flex flex-col gap-2 text-xs"><p>{progressLabel(status)}</p>{status.progress.total !== null && <progress aria-label="原图上传进度" max={Math.max(1, status.progress.total)} value={status.progress.completed} className="h-1 w-full accent-accent" />}</div>}
    <p className="text-xs text-ink-secondary">上次同步 {syncTime(status.lastSyncAt)}</p>
    {status.error && <p role="alert" className="select-text whitespace-pre-wrap break-all text-xs text-accent">{status.error}</p>}
    {waitingNote(status) && <p className="text-xs text-ink-secondary">{waitingNote(status)}</p>}
    <SyncDevices status={status} />
    <div className="flex flex-wrap gap-2 text-xs">
      {status.manualPaused
        ? <Button variant="primary" disabled={busy} onClick={() => void run(() => ipc.setSyncPaused(false))}>继续同步</Button>
        : <>
          <Button variant="primary" disabled={busy || status.phase === "syncing"} onClick={() => void run(ipc.syncNow)}>立即同步</Button>
          <Button disabled={busy} onClick={() => void run(() => ipc.setSyncPaused(true))}>暂停同步</Button>
        </>}
      <button type="button" className="px-1 text-ink-secondary underline underline-offset-4" onClick={onSettings}>同步设置…</button>
    </div>
    {error && <p role="alert" className="text-xs text-accent">{error}</p>}
  </div>, document.body);
}
