import { ArchiveIcon, ArrowCircleUpIcon, CloudIcon, SlidersHorizontalIcon, SmileyIcon, type Icon } from "@phosphor-icons/react";
import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button } from "../../components/Button";
import { Switch } from "../../components/Switch";
import { ipc, type Settings, type SyncStatus, type UpdateStatus } from "../../lib/ipc";
import { ShortcutRecorder } from "./ShortcutRecorder";
import { navigation, selectedNavigation } from "./libraryViews";
import { SyncPreferences } from "./SyncPreferences";
import { downloadLabel, focusOpener } from "./useUpdate";
import { open, confirm } from "@tauri-apps/plugin-dialog";
import { type LibraryLocation, type MoveStatus } from "../../lib/ipc";
import { useStatusEvent } from "../../lib/useStatusEvent";
import { useLibraryOverview } from "../../lib/queries";
import { startExport, exportBusy, useExportStore } from "../../stores/export";
import { storageSize, syncTime } from "./useSyncStatus";
import { ExportStatusContent } from "./ExportBar";

type SectionId = "general" | "quick" | "sync" | "library" | "update";
type BallSizeAction = { kind: "preview"; size: number | null } | { kind: "commit"; size: number };
const BALL_SIZE = { min: 40, max: 80, step: 4, default: 56 };
const SECTIONS: { id: SectionId; label: string; icon: Icon }[] = [
  { id: "general", label: "通用", icon: SlidersHorizontalIcon },
  { id: "quick", label: "悬浮球与面板", icon: SmileyIcon },
  { id: "sync", label: "同步", icon: CloudIcon },
  { id: "library", label: "仓库", icon: ArchiveIcon },
  { id: "update", label: "更新", icon: ArrowCircleUpIcon },
];

export function PreferencesDialog({ onClose, sync, focusSync = false, update, onShowUpdate }: {
  onClose: () => void; sync?: SyncStatus; focusSync?: boolean; update?: UpdateStatus; onShowUpdate: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const title = useId();
  const [section, setSection] = useState<SectionId>(focusSync ? "sync" : "general");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autoError, setAutoError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [location, setLocation] = useState<LibraryLocation | null>(null);
  const [move, setMove] = useState<MoveStatus | null>(null);
  useStatusEvent("library-location-changed", ipc.libraryLocation, setLocation, setError);
  useStatusEvent("library-move-status-changed", ipc.libraryMoveStatus, setMove, setError);
  const path = location?.path ?? "";
  const exporting = useExportStore(exportBusy);
  const exportBatch = useExportStore(s => s.status?.batch ?? 0);
  // 移动中从托盘、悬浮球菜单或 ⌘Q 退出会被拦下，这里切到「仓库」让用户看到进度和取消按钮。
  const quitRequests = move?.running ? move.quitRequests : 0;
  const overview = useLibraryOverview();
  const shortcutRecording = useRef(false);
  const [busy, setBusy] = useState<string | null>(null);
  const working = useRef(false);
  const [checkError, setCheckError] = useState<string | null>(null);
  const [draftBallSize, setDraftBallSize] = useState<number | null>(null);
  const draftBallSizeRef = useRef<number | null>(null);
  const settingsRef = useRef<Settings | null>(null);
  const pointerDragging = useRef(false);
  const dragCleanup = useRef<(() => void) | null>(null);
  const ballSizeInFlight = useRef<Promise<void> | null>(null);
  const ballSizePending = useRef<BallSizeAction[]>([]);
  const mounted = useRef(true);
  const tabId = (id: SectionId) => `${title}-tab-${id}`;
  useEffect(() => {
    mounted.current = true;
    const element = dialog.current!;
    element.showModal();
    // showModal 会把焦点给第一个可聚焦元素，这里换成当前分组的侧边选项。
    document.getElementById(tabId(section))?.focus();
    let disposed = false;
    let receivedEvent = false;
    const listening = listen<Settings>("settings-changed", ({ payload }) => {
      if (!disposed) { receivedEvent = true; settingsRef.current = payload; setSettings(payload); }
    });
    void listening.then(() => ipc.getSettings()).then((s) => {
      if (!disposed && !receivedEvent) { settingsRef.current = s; setSettings(s); }
    })
      .catch((e: unknown) => { if (!disposed) setError(String(e)); });
    void ipc.getAutostart().then((enabled) => { if (!disposed) setAutostart(enabled); })
      .catch((e: unknown) => { if (!disposed) setAutoError(String(e)); });
    return () => {
      disposed = true;
      mounted.current = false;
      dragCleanup.current?.();
      dragCleanup.current = null;
      if (draftBallSizeRef.current !== null || pointerDragging.current) {
        pointerDragging.current = false;
        draftBallSizeRef.current = null;
        enqueueBallSize({ kind: "preview", size: null });
      }
      element.close();
      void listening.then((cleanup) => cleanup(), () => {});
      // 关闭即算已读：清掉上次移动的结果和错误，下次打开不再冒出来。移动中后端不会清。
      void ipc.clearLibraryMoveNotice().catch(() => {});
    };
  }, []);
  useEffect(() => { if (quitRequests > 0) select("library"); }, [quitRequests]);

  function moveLibrary(restore: boolean) {
    return change("move", async () => {
      const path = restore ? location?.defaultPath : await open({ title: "选择新的仓库位置", directory: true, multiple: false, canCreateDirectories: true });
      if (!path) return;
      if (move?.report || move?.error) await ipc.clearLibraryMoveNotice();
      const target = await ipc.inspectLibraryTarget(path);
      if (await confirm(`仓库（${storageSize(target.bytes)}，${target.files} 个文件）会复制到：
${target.path}

检查无误后会切换并删除原位置的仓库文件。期间同步会暂停，导入、导出和退出应用不可用。请不要断开这个磁盘。`, {
        title: restore ? "恢复默认位置" : "移动仓库", kind: "warning", okLabel: "移动", cancelLabel: "取消",
      })) await ipc.moveLibrary(path).catch(async (reason: unknown) => {
        // 移动过程中的失败已记进移动状态，在「仓库」分组里显示；没开始就被拒（比如正在导入）才放到底部。
        const current = await ipc.libraryMoveStatus().catch(() => null);
        if (current?.error !== String(reason)) throw reason;
      });
    });
  }
  function select(id: SectionId) {
    setSection(id);
    if (content.current) content.current.scrollTop = 0;
  }
  function moveTab(event: KeyboardEvent<HTMLDivElement>) {
    const index = SECTIONS.findIndex(({ id }) => id === section);
    const last = SECTIONS.length - 1;
    const next = event.key === "ArrowDown" ? (index === last ? 0 : index + 1)
      : event.key === "ArrowUp" ? (index === 0 ? last : index - 1)
      : event.key === "Home" ? 0 : event.key === "End" ? last : null;
    if (next === null) return;
    event.preventDefault();
    select(SECTIONS[next].id);
    document.getElementById(tabId(SECTIONS[next].id))?.focus();
  }

  async function change(kind: string, operation: () => Promise<unknown>) {
    if (working.current) return;
    working.current = true; setBusy(kind); setError(null);
    try { await operation(); }
    catch (reason) {
      setError(String(reason));
      try {
        const current = await ipc.getSettings();
        settingsRef.current = current;
        setSettings(current);
      } catch { /* 保留原始操作错误 */ }
    } finally { working.current = false; setBusy(null); }
  }
  // 只合并队尾尚未执行的预览；松手后的提交必须保留，后续预览或取消不能覆盖它。
  function enqueueBallSize(action: BallSizeAction) {
    const pending = ballSizePending.current;
    if (pending.at(-1)?.kind === "preview") pending.pop();
    pending.push(action);
    if (ballSizeInFlight.current) return;
    const run = async () => {
      while (pending.length) {
        const next = pending.shift()!;
        try {
          if (next.kind === "preview") {
            await ipc.previewBallSize(next.size);
          } else {
            const current = settingsRef.current;
            if (!current || next.size === current.ballSize) {
              await ipc.previewBallSize(null);
            } else {
              const saved = await ipc.setBallSize(next.size);
              settingsRef.current = saved;
              if (mounted.current) setSettings(saved);
            }
          }
        } catch (reason) {
          if (mounted.current) setError(String(reason));
          if (next.kind === "commit") {
            try {
              const current = await ipc.getSettings();
              settingsRef.current = current;
              if (mounted.current) setSettings(current);
            } catch { /* 保留原始操作错误 */ }
            await ipc.previewBallSize(null).catch(() => {});
          }
        } finally {
          if (next.kind === "commit" && !pending.length && !pointerDragging.current) {
            draftBallSizeRef.current = null;
            if (mounted.current) setDraftBallSize(null);
          }
        }
      }
    };
    const task = run().finally(() => {
      if (ballSizeInFlight.current === task) ballSizeInFlight.current = null;
    });
    ballSizeInFlight.current = task;
  }
  function queueBallSizePreview(value: number | null) {
    enqueueBallSize({ kind: "preview", size: value });
  }
  function commitBallSize(size: number) {
    enqueueBallSize({ kind: "commit", size });
  }
  function cancelBallSizePreview() {
    dragCleanup.current?.();
    dragCleanup.current = null;
    pointerDragging.current = false;
    if (draftBallSizeRef.current !== null || ballSizePending.current.length || ballSizeInFlight.current) {
      draftBallSizeRef.current = null;
      setDraftBallSize(null);
      enqueueBallSize({ kind: "preview", size: null });
    }
  }
  function startBallSizeDrag() {
    dragCleanup.current?.();
    pointerDragging.current = true;
    const finish = () => {
      dragCleanup.current?.();
      dragCleanup.current = null;
      pointerDragging.current = false;
      if (draftBallSizeRef.current !== null) {
        commitBallSize(draftBallSizeRef.current);
      }
    };
    window.addEventListener("pointerup", finish, { once: true });
    window.addEventListener("pointercancel", finish, { once: true });
    dragCleanup.current = () => {
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
    };
  }
  const rawBallSize = Math.min(BALL_SIZE.max, Math.max(BALL_SIZE.min, settings?.ballSize ?? BALL_SIZE.default));
  const savedBallSize = Math.min(
    BALL_SIZE.max,
    BALL_SIZE.min + Math.round((rawBallSize - BALL_SIZE.min) / BALL_SIZE.step) * BALL_SIZE.step,
  );
  const displayBallSize = draftBallSize ?? savedBallSize;
  async function toggleAutostart() {
    setAutoError(null);
    try {
      const result = await ipc.setAutostart(!autostart);
      setAutostart(result.enabled); setAutoError(result.error);
    } catch (reason) {
      setAutoError(String(reason));
      try { setAutostart(await ipc.getAutostart()); } catch { setAutostart(null); }
    }
  }
  const split = Math.max(0, path.length - 28);
  // 分组只是隐藏不卸载：同步表单填到一半切走再切回来，内容和进行中的连接都要还在。
  const page = (id: SectionId) => ({ prefix: title, id, active: section === id });
  return <dialog ref={dialog} aria-labelledby={title} onCancel={(event) => { event.preventDefault(); if (move?.running || busy === "move" || shortcutRecording.current) return; cancelBallSizePreview(); onClose(); }}
    className="m-auto h-140 max-h-[calc(100%-48px)] w-180 max-w-[calc(100%-48px)] overflow-hidden rounded-card border border-line bg-surface text-ink backdrop:bg-overlay">
    <div className="flex h-full">
      <div className="flex w-44 shrink-0 flex-col gap-4 bg-chrome px-3 pt-6">
        <h2 id={title} className="px-2 text-lg font-semibold">偏好设置</h2>
        <div role="tablist" aria-orientation="vertical" aria-labelledby={title} onKeyDown={moveTab} className="flex flex-col gap-0.5">
          {SECTIONS.map(({ id, label, icon: SectionIcon }) => <button key={id} id={tabId(id)} type="button" role="tab"
            aria-selected={section === id} aria-controls={`${title}-page-${id}`} tabIndex={section === id ? 0 : -1} onClick={() => select(id)}
            className={`${navigation} ${section === id ? selectedNavigation : "font-medium"}`}>
            <SectionIcon size={16} aria-hidden /><span className="min-w-0 flex-1 truncate">{label}</span>
          </button>)}
        </div>
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        <div ref={content} className="min-h-0 flex-1 overflow-y-auto p-6">
          <Page {...page("general")}>
            <Group>
              <div className="flex flex-wrap items-center justify-between gap-4">
                <span id={`${title}-appearance`}>外观</span>
                <div role="group" aria-labelledby={`${title}-appearance`} className="flex rounded-control bg-muted p-1">
                  {([['system', '跟随系统'], ['light', '浅色'], ['dark', '深色']] as const).map(([value, label]) =>
                    <button key={value} type="button" aria-pressed={settings?.appearance === value}
                      disabled={!!busy || !settings}
                      onClick={() => void change("appearance", async () => setSettings(await ipc.setAppearance(value)))}
                      className="interactive rounded-control px-3 py-1 text-xs font-semibold aria-pressed:bg-surface hover:opacity-82 disabled:opacity-50">{label}</button>)}
                </div>
              </div>
              <div className="flex items-center justify-between gap-4">
                <span id={`${title}-autostart`}>开机时自动启动</span>
                <Switch aria-labelledby={`${title}-autostart`} aria-describedby={autoError ? `${title}-auto-error` : undefined}
                  checked={autostart === true} disabled={!!busy || autostart === null} onClick={() => void change("autostart", toggleAutostart)} />
              </div>
              {autoError && <p id={`${title}-auto-error`} role="alert" className="text-xs text-accent">{autoError}</p>}
            </Group>
          </Page>
          <Page {...page("quick")}>
            <Group title="悬浮球">
              <div className="flex items-center justify-between gap-4">
                <span id={`${title}-ball`}>显示悬浮球</span>
                <Switch aria-labelledby={`${title}-ball`} checked={settings?.ballVisible ?? false} disabled={!!busy || !settings}
                  onClick={() => void change("ball", async () => setSettings(await ipc.setBallVisible(!settings!.ballVisible)))} />
              </div>
              <label className={`flex items-center justify-between gap-4 ${settings?.ballVisible ? "" : "text-ink-secondary"}`}>
                <span>静止后收到屏幕边缘</span>
                <select value={settings?.ballAutoHideSeconds ?? "never"} disabled={!!busy || !settings?.ballVisible}
                  onChange={(event) => { const seconds = event.target.value === "never" ? null : Number(event.target.value) as 3 | 10;
                    void change("auto-hide", async () => setSettings(await ipc.setBallAutoHide(seconds))); }}
                  className="rounded-control border border-line bg-surface px-3 py-1 disabled:opacity-50">
                  <option value={3}>3 秒</option><option value={10}>10 秒</option><option value="never">不收起</option>
                </select>
              </label>
              <div className={`flex items-center justify-between gap-4 ${settings?.ballVisible ? "" : "text-ink-secondary"}`}>
                <label htmlFor={`${title}-ball-size`}>悬浮球大小</label>
                <div className="flex items-center gap-3">
                  {displayBallSize !== BALL_SIZE.default && (
                    <button type="button" disabled={!!busy || !settings?.ballVisible}
                      onClick={() => {
                        draftBallSizeRef.current = BALL_SIZE.default;
                        setDraftBallSize(BALL_SIZE.default);
                        setError(null);
                        commitBallSize(BALL_SIZE.default);
                      }}
                      className="text-xs text-ink-secondary underline underline-offset-4 hover:text-ink disabled:opacity-50">恢复默认</button>
                  )}
                  <input id={`${title}-ball-size`} type="range" aria-valuetext={`${displayBallSize} 像素`}
                    min={BALL_SIZE.min} max={BALL_SIZE.max} step={BALL_SIZE.step} value={displayBallSize}
                    disabled={!!busy || !settings?.ballVisible}
                    onPointerDown={startBallSizeDrag}
                    onChange={(event) => {
                      const next = Number(event.target.value);
                      draftBallSizeRef.current = next;
                      setDraftBallSize(next);
                      setError(null);
                      queueBallSizePreview(next);
                    }}
                    onKeyUp={() => {
                      if (!pointerDragging.current && draftBallSizeRef.current !== null) {
                        commitBallSize(draftBallSizeRef.current);
                      }
                    }}
                    onBlur={() => {
                      if (!pointerDragging.current && draftBallSizeRef.current !== null) {
                        commitBallSize(draftBallSizeRef.current);
                      }
                    }}
                    className="tile-slider interactive w-32 hover:opacity-82 disabled:opacity-50" />
                  <span className="w-11 text-right text-xs tabular-nums text-ink-secondary">{displayBallSize} px</span>
                </div>
              </div>
            </Group>
            <Group title="表情面板">
              <ShortcutRecorder active={section === "quick"} onRecordingChange={(v) => { shortcutRecording.current = v; }} />
            </Group>
          </Page>
          <Page {...page("sync")}><SyncPreferences status={sync} /></Page>
          <Page {...page("library")}>
            <Group>
              <span className="text-xs text-ink-secondary">仓库路径</span>
              <p title={path} tabIndex={0} className="flex min-w-0 select-text rounded-sm text-xs"
                onCopy={(event) => { event.clipboardData.setData("text/plain", path); event.preventDefault(); }}>
                <span className="min-w-0 truncate">{path.slice(0, split)}</span><span className="shrink-0">{path.slice(split)}</span>
              </p>
              {location?.isCustom && <p className="select-text break-all text-xs text-ink-secondary">默认位置：{location.defaultPath}</p>}
              <div className="flex flex-wrap gap-2">
                <Button disabled={!!busy || move?.running || !path} onClick={() => void change("directory", ipc.openLibraryDirectory)}>打开所在位置</Button>
                <Button disabled={!!busy || move?.running || !location?.canMove || exporting} onClick={() => void moveLibrary(false)}>移动位置…</Button>
                {location?.isCustom && <Button disabled={!!busy || move?.running || !location.canMove || exporting} onClick={() => void moveLibrary(true)}>恢复默认位置</Button>}
              </div>
              <p className="text-xs text-ink-secondary">{location?.cannotMoveReason ?? "请选择本机磁盘上的空文件夹，避免网盘同步文件夹和网络驱动器。移动时会复制全部表情和本机设置，检查无误后才切换，再删除原位置的仓库文件。"}</p>
              {quitRequests > 0 && <p role="alert" className="text-xs text-accent">正在移动仓库，请等移动完成或取消后再退出。</p>}
              {move?.running && <div role="status" className="flex flex-wrap items-center gap-3 text-xs">
                {move.stage === "preparing" && <span>正在等同步停下…</span>}
                {move.stage === "switching" && <span>正在切换到新位置…</span>}
                {move.stage === "cleaning" && <span>正在清理原位置…</span>}
                {move.stage === "copying" && <>
                  <span className="shrink-0">正在复制 {move.progress.copied}/{move.progress.total} · {storageSize(move.progress.bytes)} / {storageSize(move.progress.totalBytes)}</span>
                  <progress aria-label="移动进度" value={move.progress.total ? move.progress.copied : undefined} max={Math.max(1, move.progress.total)} className="h-1 min-w-16 max-w-80 flex-1 accent-accent" />
                  <button type="button" className="shrink-0 underline underline-offset-4" onClick={() => void ipc.cancelLibraryMove().catch(e => setError(String(e)))}>取消</button>
                </>}
              </div>}
              {move?.report && <p role="status" className="select-text text-xs">仓库已移到 {move.report.path}
                {!!move.report.leftovers.length && <><br/>原位置还有 {move.report.leftovers.length} 项没能删除，可以手动删除：{move.report.previous}</>}</p>}
              {move?.error && <p role="alert" className="text-xs text-accent">{move.error}</p>}
            </Group>
            <Group title="导出仓库">
              <p className="text-xs text-ink-secondary">把全部表情按分类导出成图片文件。最近删除里的表情，以及配文、收藏和使用次数不会导出。</p>
              <div className="flex flex-wrap gap-2">{(["folder", "zip"] as const).map(format =>
                <Button key={format} disabled={!!busy || !overview.data?.all || exporting || move?.running} onClick={() => void startExport("library", format)}>
                  {format === "folder" ? "导出到文件夹…" : "导出为 .zip…"}
                </Button>)}</div>
              <ExportStatusContent key={exportBatch} inline />
            </Group>
          </Page>
          <Page {...page("update")}>
            <Group>
              <div className="flex items-center justify-between gap-4">
                <span>当前版本 {update?.currentVersion ?? ""}</span>
                <Button disabled={!update || update.phase !== "idle"} onClick={() => { setCheckError(null); void ipc.checkUpdate().catch((e: unknown) => setCheckError(String(e))); }}>
                  {update?.phase === "checking" ? "正在检查…" : "检查更新"}
                </Button>
              </div>
              {update && <UpdateNote status={update} onShow={onShowUpdate} />}
              {checkError && <p role="alert" className="text-xs text-accent">{checkError}</p>}
              <div className="flex items-center justify-between gap-4">
                <span id={`${title}-auto-update`}>自动检查更新</span>
                <Switch aria-labelledby={`${title}-auto-update`} aria-describedby={`${title}-auto-update-note`} checked={settings?.autoCheckUpdates ?? false} disabled={!!busy || !settings}
                  onClick={() => void change("auto-update", async () => setSettings(await ipc.setAutoCheckUpdates(!settings!.autoCheckUpdates)))} />
              </div>
              <p id={`${title}-auto-update-note`} className="text-xs text-ink-secondary">启动时和每天检查一次，发现新版本只提示，确认后才下载</p>
            </Group>
          </Page>
        </div>
        <div className="flex items-center justify-end gap-4 px-6 pb-6">
          {error && <p role="alert" className="min-w-0 flex-1 text-xs text-accent">{error}</p>}
          <Button variant="primary" disabled={move?.running || busy === "move"} onClick={() => {
            if (!pointerDragging.current && draftBallSizeRef.current !== null) {
              const finalSize = draftBallSizeRef.current;
              draftBallSizeRef.current = null;
              commitBallSize(finalSize);
            }
            onClose();
          }}>完成</Button>
        </div>
      </div>
    </div>
  </dialog>;
}
function UpdateNote({ status, onShow }: { status: UpdateStatus; onShow: () => void }) {
  if (status.phase === "downloading" || status.phase === "installing") return <p role="status" className="text-xs">{downloadLabel(status)}</p>;
  if (status.error) return <p role="alert" className="text-xs text-accent">{status.error}</p>;
  if (status.available) return <div className="flex items-center justify-between gap-4 text-xs">
    <span role="status">发现新版本 {status.available.version}{status.ignored ? "（已忽略）" : ""}</span>
    <button type="button" onClick={(event) => { focusOpener(event.currentTarget); onShow(); }} className="shrink-0 underline underline-offset-4">查看</button>
  </div>;
  if (status.lastChecked && status.phase === "idle") return <p role="status" className="text-xs text-ink-secondary">已是最新版本 · 检查于 {syncTime(status.lastChecked)}</p>;
  return null;
}
function Page({ prefix, id, active, children }: { prefix: string; id: SectionId; active: boolean; children: ReactNode }) {
  return <section role="tabpanel" id={`${prefix}-page-${id}`} aria-labelledby={`${prefix}-tab-${id}`} hidden={!active} className="flex flex-col gap-6">
    <h3 className="text-lg font-semibold">{SECTIONS.find((section) => section.id === id)?.label}</h3>
    {children}
  </section>;
}
function Group({ title, children }: { title?: string; children: ReactNode }) {
  return <div className="flex flex-col gap-3">{title && <h4 className="text-xs font-semibold text-ink-secondary">{title}</h4>}{children}</div>;
}
