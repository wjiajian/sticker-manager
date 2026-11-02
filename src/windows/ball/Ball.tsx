import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ipc, type SyncStatus, type BallSnapshot } from "../../lib/ipc";
import { formatReport } from "../../lib/importReport";
import { progressLabel } from "../main/useSyncStatus";
import { platform } from "../../lib/platform";

export function Ball() {
  const [sync, setSync] = useState<SyncStatus | null>(null);
  const [ring, setRing] = useState<SyncStatus["progress"]>(null);
  useEffect(() => {
    if (sync?.progress) { setRing(sync.progress); return; }
    const timer = setTimeout(() => setRing(null), 300);
    return () => clearTimeout(timer);
  }, [sync?.progress]);
  const [state, setState] = useState<BallSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  // 操作失败和导入结果一样在球旁显示约 2 秒；只有读不到初始状态时才一直显示错误。
  function showError(reason: unknown) {
    void ipc.ballMessage(String(reason)).catch(() => setError(String(reason)));
  }
  useEffect(() => {
    let disposed = false;
    // 原生窗口大小、球心偏移和外观状态一起从 Rust 更新。
    const listeners = [
      listen<SyncStatus>("sync-status-changed", ({ payload }) => { if (!disposed) setSync(payload); }),
      listen<BallSnapshot>("ball-state", ({ payload }) => { if (!disposed) setState(payload); }),
      listen<boolean>("panel-visibility", ({ payload }) => { if (!disposed) setState((s) => s && { ...s, panelVisible: payload }); }),
    ];
    void Promise.all(listeners).then(() => ipc.ballState()).then((s) => { if (!disposed) setState(s); })
      .catch((reason: unknown) => { if (!disposed) setError(String(reason)); });
    void ipc.syncStatus().then((value) => { if (!disposed) setSync(value); }).catch(showError);
    const drop = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (disposed) return;
      if (payload.type === "enter") void ipc.setBallDrop(payload.paths).catch(reportError);
      else if (payload.type === "leave") void ipc.setBallDrop(null).catch(reportError);
      else if (payload.type === "drop") {
        void ipc.setBallDrop(null).catch(reportError);
        void (async () => {
          if (!payload.paths.length) return;
          try { await ipc.ballMessage(formatReport(await ipc.importStickers(payload.paths))); }
          catch (reason) { await ipc.ballMessage(`导入失败：${String(reason)}`); }
        })().catch(reportError);
      }
    });
    function reportError(reason: unknown) { if (!disposed) showError(reason); }
    void drop.catch(reportError);
    return () => {
      disposed = true;
      for (const pending of [...listeners, drop]) void pending.then((cleanup) => cleanup(), () => {});
    };
  }, []);
  async function run(operation: () => Promise<unknown>) {
    try { await operation(); }
    catch (reason) { showError(reason); }
  }
  if (!state) return error ? <p role="alert" className="bg-surface p-2 text-xs text-accent">{error}</p> : null;
  const dropping = !!state.dropLabel;
  const size = dropping ? 72 : 56;
  const tip = state.dropLabel ?? state.message ?? (state.hovered && !state.dragging ? (progressLabel(sync ?? undefined) ? `${progressLabel(sync ?? undefined)} · 单击展开` : "单击展开 · 右键更多") : null);
  const eyes = state.collapsed ? "M21 31h8M35 31h8" : state.hovered || dropping ? "M25 29v3M39 29v3" : "M21 31q4 -5 8 0M35 31q4 -5 8 0";
  const mouth = state.collapsed ? "M27 43h10" : dropping ? "M26 40q6 10 12 0Z" : "M24 41q8 8 16 0";
  return <main aria-label="悬浮球" className="relative h-full w-full overflow-hidden">
    <button type="button" tabIndex={-1} aria-label={state.panelVisible ? "收起表情面板" : "展开表情面板"} aria-expanded={state.panelVisible}
      className="interactive absolute touch-none rounded-full bg-surface p-2 outline-none"
      style={{ left: state.center.x - size / 2, top: state.center.y - size / 2, width: size, height: size,
        opacity: state.collapsed ? 0.7 : 1, transform: `scale(${state.pressed ? 0.96 : state.hovered && !dropping ? 1.1 : 1})`,
        boxShadow: `var(--shadow-ball)${state.panelVisible ? ", inset 0 0 0 3px var(--color-accent)" : ""}`, cursor: state.dragging ? "grabbing" : "pointer" }}
      onPointerDown={(event) => {
        event.preventDefault();
        // macOS 上 Control+单击是右键菜单，只交给 onContextMenu。
        if (event.button === 0 && !(platform === "macos" && event.ctrlKey)) void run(ipc.ballPointerDown);
      }}
      onContextMenu={(event) => { event.preventDefault(); void run(ipc.showBallMenu); }}>
      <svg viewBox="0 0 64 64" className="block h-full w-full" aria-hidden>
        <circle cx="32" cy="34" r="24" fill="var(--color-brand)" stroke="var(--color-brand-ink)" strokeWidth="3" />
        <path d={eyes} fill="none" stroke="var(--color-brand-ink)" strokeWidth="3.5" strokeLinecap="round" />
        <path d={mouth} fill="none" stroke="var(--color-brand-ink)" strokeWidth="3.5" strokeLinecap="round" />
      </svg>
      {dropping && <span aria-hidden className="pointer-events-none absolute -inset-3 rounded-full border-2 border-dashed border-accent" />}
    </button>
    {ring && <svg aria-hidden viewBox="0 0 68 68" className="pointer-events-none absolute transition-opacity duration-300" style={{ left: state.center.x - 34, top: state.center.y - 34, width: 68, height: 68, opacity: sync?.progress ? 1 : 0 }}>
      <circle cx="34" cy="34" r="32.5" fill="none" stroke="var(--color-ball-track)" strokeWidth="3" />
      <circle cx="34" cy="34" r="32.5" fill="none" stroke="var(--color-accent)" strokeWidth="3" strokeLinecap="round" pathLength="100"
        strokeDasharray={ring.total ? `${Math.max(0, Math.min(100, ring.completed / ring.total * 100))} 100` : "25 100"}
        className={ring.total === null ? "sync-download-arc" : undefined} style={{ transformOrigin: "34px 34px", transform: "rotate(-90deg)" }} />
    </svg>}
    {!!sync?.unseen && <span aria-label={`${sync.unseen} 张新表情`} className="pointer-events-none absolute min-w-6 rounded-full bg-accent-fill px-1 py-0.5 text-center text-xs leading-4 font-bold text-on-accent shadow-[0_0_0_2px_var(--color-surface)]"
      style={{ top: Math.max(2, state.center.y - 32), ...(state.edge === "right" ? { right: Math.max(2, 300 - state.center.x + 4) } : { left: Math.max(2, state.center.x + 4) }) }}>{sync.unseen > 99 ? "99+" : sync.unseen}</span>}
    {tip && !state.collapsed && <div role="status" className="pointer-events-none absolute max-w-72 rounded-control bg-ink px-2 py-1 text-xs text-surface"
      style={{ ...(state.edge === "right" ? { right: 8 } : { left: 8 }),
        ...(state.tipAbove ? { bottom: `calc(100% - ${state.center.y - 56}px)` } : { top: state.center.y + 56 }) }}>
      <span className="line-clamp-3 break-all">{tip}</span>
    </div>}
  </main>;
}
