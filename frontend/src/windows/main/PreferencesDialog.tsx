import { CheckIcon, MagnifyingGlassIcon, XCircleIcon, XIcon } from "@phosphor-icons/react";
import { listen } from "@tauri-apps/api/event";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { SegmentedControl } from "../../components/SegmentedControl";
import { Switch } from "../../components/Switch";
import { ipc, type LibraryLocation, type MoveStatus, type Settings, type SyncStatus, type UpdateStatus } from "../../lib/ipc";
import { usePanelShortcut } from "../../lib/panelShortcut";
import { platform } from "../../lib/platform";
import { useLibraryOverview } from "../../lib/queries";
import { useStatusEvent } from "../../lib/useStatusEvent";
import { exportBusy, exportDismissed, startExport, useExportStore } from "../../stores/export";
import { BallSizeSetting } from "./BallSizeSetting";
import { ExportStatusContent } from "./ExportBar";
import { LibrarySettings } from "./LibrarySettings";
import { ControlSkeleton, InlineError, PreferencesFilterContext, PrefSection, SettingRow } from "./PreferencesLayout";
import { failure, percentage, PreferencesNav, type NavStatus } from "./PreferencesNav";
import { matchPreferences, type SectionId } from "./preferencesSections";
import { ShortcutRecorder } from "./ShortcutRecorder";
import { SyncPreferences } from "./SyncPreferences";
import { UpdateSettings } from "./UpdateSettings";
import { useBallSize } from "./useBallSize";
import { useScrollSpy } from "./useScrollSpy";
import { storageSize, syncLabel } from "./useSyncStatus";

const appearanceOptions = [
  { value: "system", label: "跟随系统" }, { value: "light", label: "浅色" }, { value: "dark", label: "深色" },
] as const;
const autoHideOptions = [
  { value: 3, label: "3 秒" }, { value: 10, label: "10 秒" }, { value: "never", label: "不收起" },
] as const;
const sectionFor = {
  appearance: "appearance", autostart: "startup", ball: "ball", "auto-hide": "ball", directory: "library", move: "library", "auto-update": "update",
} as const;

export function PreferencesDialog({ onClose, sync, syncReadError, focusSync = false, update, updateReadError, onShowUpdate }: {
  onClose: () => void; sync?: SyncStatus; syncReadError?: string; updateReadError?: string; focusSync?: boolean; update?: UpdateStatus; onShowUpdate: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const composing = useRef(false);
  const title = useId();
  const [input, setInput] = useState("");
  const [query, setQuery] = useState("");
  const clearQuery = useCallback(() => { setInput(""); setQuery(""); }, []);
  const matched = useMemo(() => matchPreferences(query), [query]);
  const { active, jumpTo } = useScrollSpy(content, query, clearQuery);
  const [settings, setSettings] = useState<Settings | null>(null);
  const settingsRef = useRef<Settings | null>(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autoError, setAutoError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [sectionErrors, setSectionErrors] = useState<Partial<Record<SectionId, string>>>({});
  const setError = useCallback((id: SectionId, message: string | null) => {
    setSectionErrors(previous => ({ ...previous, [id]: message ?? undefined }));
  }, []);
  const libraryError = useCallback((message: string | null) => setError("library", message), [setError]);
  const [location, setLocation] = useState<LibraryLocation | null>(null);
  const [move, setMove] = useState<MoveStatus | null>(null);
  useStatusEvent("library-location-changed", ipc.libraryLocation, setLocation, libraryError);
  useStatusEvent("library-move-status-changed", ipc.libraryMoveStatus, setMove, libraryError);
  const overview = useLibraryOverview();
  const shortcut = usePanelShortcut();
  const shortcutRecording = useRef(false);
  const exports = useExportStore();
  const exporting = exportBusy(exports);
  const [busy, setBusy] = useState<keyof typeof sectionFor | null>(null);
  const working = useRef(false);
  const [checkError, setCheckError] = useState<string | null>(null);
  const [savedFlash, setSavedFlash] = useState(false);
  const flashTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const mounted = useRef(false);
  function flashSaved() {
    if (!mounted.current) return;
    clearTimeout(flashTimer.current);
    setSavedFlash(true);
    flashTimer.current = setTimeout(() => setSavedFlash(false), 2000);
  }
  const ball = useBallSize(settings, settingsRef, setSettings, message => setError("ball", message));
  useEffect(() => {
    mounted.current = true;
    const element = dialog.current!;
    element.showModal();
    if (focusSync) jumpTo("sync", { focus: true, smooth: false });
    else search.current?.focus();
    let disposed = false;
    let receivedEvent = false;
    let lastBallSize: number | undefined;
    const listening = listen<Settings>("settings-changed", ({ payload }) => {
      if (disposed) return;
      receivedEvent = true;
      if (lastBallSize !== undefined && lastBallSize !== payload.ballSize) flashSaved();
      lastBallSize = payload.ballSize;
      settingsRef.current = payload;
      setSettings(payload);
      setLoadError(null);
    });
    void listening.then(() => ipc.getSettings()).then(s => {
      if (!disposed && !receivedEvent) { lastBallSize = s.ballSize; settingsRef.current = s; setSettings(s); }
    }).catch(e => { if (!disposed && !receivedEvent) setLoadError(String(e)); });
    void ipc.getAutostart().then(value => { if (!disposed) setAutostart(value); })
      .catch(e => { if (!disposed) setAutoError(String(e)); });
    return () => {
      disposed = true;
      mounted.current = false;
      clearTimeout(flashTimer.current);
      element.close();
      void listening.then(cleanup => cleanup(), () => {});
      void ipc.clearLibraryMoveNotice().catch(() => {});
    };
  }, []);
  const quitRequests = move?.running ? move.quitRequests : 0;
  const handledQuit = useRef(0);
  useLayoutEffect(() => {
    if (!quitRequests) { handledQuit.current = 0; return; }
    if (handledQuit.current === quitRequests) return;
    // Even a path-only search hides the move notice. Jump after the filter is cleared.
    if (input || query) { clearQuery(); return; }
    handledQuit.current = quitRequests;
    jumpTo("library", { focus: true, smooth: true });
  }, [quitRequests, input, query, clearQuery, jumpTo]);

  async function change(kind: keyof typeof sectionFor, operation: () => Promise<unknown>) {
    if (working.current) return;
    working.current = true;
    setBusy(kind);
    setError(sectionFor[kind], null);
    try {
      await operation();
      if (kind !== "directory" && kind !== "move" && kind !== "autostart") flashSaved();
    } catch (reason) {
      setError(sectionFor[kind], String(reason));
      try {
        const current = await ipc.getSettings();
        settingsRef.current = current;
        setSettings(current);
      } catch { /* Keep the original error. */ }
    } finally {
      working.current = false;
      setBusy(null);
    }
  }
  async function toggleAutostart() {
    setAutoError(null);
    try {
      const result = await ipc.setAutostart(!autostart);
      setAutostart(result.enabled);
      setAutoError(result.error);
      if (!result.error) flashSaved();
    } catch (reason) {
      setAutoError(String(reason));
      try { setAutostart(await ipc.getAutostart()); } catch { setAutostart(null); }
    }
  }
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
        // 后端已记录的移动失败由进度区域显示。
        const current = await ipc.libraryMoveStatus().catch(() => null);
        if (current?.error !== String(reason)) throw reason;
      });
    });
  }

  const moving = !!move?.running || busy === "move";
  const loading = !settings && !loadError;
  const reading = loading || (autostart === null && !autoError) || (!location && !sectionErrors.library) || (!sync && !syncReadError) || (!update && !updateReadError);
  function requestClose() {
    if (moving) return;
    ball.commitDraftBeforeClose();
    onClose();
  }
  function cancelDialog() {
    if (moving || shortcutRecording.current) return;
    if (document.activeElement === search.current && input) { clearAndFocus(); return; }
    ball.cancelPreview();
    onClose();
  }
  function clearAndFocus() { clearQuery(); search.current?.focus(); }
  const state = shortcut.data;
  const statuses: Record<SectionId, NavStatus> = {
    appearance: { text: settings?.appearance === "system" ? "系统" : appearanceOptions.find(option => option.value === settings?.appearance)?.label },
    startup: autoError ? failure : { text: autostart === null ? "" : autostart ? "开启" : "关闭" },
    ball: { text: settings && (settings.ballVisible ? `${ball.displayBallSize} px` : "已隐藏") },
    panel: shortcut.error ? failure : state?.status === "failed" ? { text: "失败", error: true } : { text: state?.status === "disabled" ? "已停用" : state?.label },
    sync: syncReadError ? failure : !sync ? {} : sync.phase === "error" ? { ...failure, dot: "bg-accent" } : !sync.target ? { text: "未设置" }
      : { text: sync.phase === "syncing" && sync.progress?.total ? percentage(sync.progress.completed, sync.progress.total) : syncLabel(sync),
          dot: sync.phase === "synced" ? "bg-synced" : "bg-ink-secondary" },
    library: move?.error ? failure : { text: location?.isCustom ? "自定义" : overview.data && `${overview.data.all} 张` },
    export: exports.status?.running ? { text: `${exports.status.progress.processed}/${exports.status.progress.total}` }
      : exports.error || (!exportDismissed(exports) && (exports.status?.error || exports.status?.report?.failures.length)) ? failure : {},
    update: updateReadError || checkError || update?.error ? failure : update?.available && !update.ignored ? { text: "新版本", dot: "bg-accent" } : {},
  };
  for (const id of Object.keys(sectionErrors) as SectionId[]) if (sectionErrors[id]) statuses[id] = id === "sync" ? { ...failure, dot: "bg-accent" } : failure;
  if (moving) statuses.library = { text: move?.stage === "copying" ? percentage(move.progress.copied, move.progress.total) : "移动中" };
  if (update?.phase === "installing" || update?.phase === "downloading") {
    statuses.update = { text: update.phase === "installing" ? "100%" : update.total ? percentage(update.downloaded, update.total) : "下载中" };
  }
  const page = (id: SectionId) => ({ prefix: title, id, error: id === "sync" ? undefined : sectionErrors[id] });
  return <dialog ref={dialog} aria-labelledby={title} aria-describedby={`${title}-save`}
    className="m-auto h-150 max-h-[calc(100%-48px)] w-190 max-w-[calc(100%-48px)] overflow-clip rounded-card border border-line bg-surface text-ink tabular-nums backdrop:bg-overlay"
    onKeyDown={event => {
      if (shortcutRecording.current || event.nativeEvent.isComposing || event.keyCode === 229 || event.altKey || event.shiftKey) return;
      const modifier = platform === "macos" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
      if (modifier && event.key.toLowerCase() === "f") { event.preventDefault(); search.current?.focus(); search.current?.select(); }
    }}
    onCancel={event => { event.preventDefault(); cancelDialog(); }}>
    <PreferencesFilterContext.Provider value={matched}>
      <div className="grid h-full grid-cols-[192px_minmax(0,1fr)] grid-rows-[64px_minmax(0,1fr)]">
        <header className="flex items-center bg-chrome px-5">
          <h2 id={title} className="text-lg font-semibold">偏好设置</h2>
        </header>
        <div className="flex items-center gap-4 px-6">
          <div className="flex min-w-0 max-w-72 flex-1 items-center gap-2 rounded-control bg-muted pr-1 pl-3 focus-within:outline-2 focus-within:outline-accent">
            <MagnifyingGlassIcon size={16} aria-hidden className="shrink-0 text-ink-secondary" />
            <input ref={search} type="search" aria-label="搜索设置" placeholder="搜索设置…" value={input}
              className="min-w-0 flex-1 appearance-none py-2 outline-none! [&::-webkit-search-cancel-button]:hidden"
              onChange={event => { setInput(event.target.value); if (!composing.current) setQuery(event.target.value); }}
              onCompositionStart={() => { composing.current = true; }}
              onCompositionEnd={event => { composing.current = false; setQuery(event.currentTarget.value); }}
              onKeyDown={event => {
                if (event.nativeEvent.isComposing || composing.current || event.keyCode === 229) return;
                if (event.key === "Escape" && input) {
                  // A native search input may clear itself before the dialog gets its cancel event.
                  event.preventDefault();
                  cancelDialog();
                }
                if (event.key === "Enter") {
                  event.preventDefault();
                  const first = matched.sections.values().next().value;
                  if (first) jumpTo(first, { focus: true, smooth: true });
                }
              }} />
            {input ? <button type="button" aria-label="清空搜索" title="清空搜索" onClick={clearAndFocus} className="interactive rounded-inset p-1 hover:bg-hover active:scale-98">
              <XCircleIcon size={16} aria-hidden /></button> : <span aria-hidden className="pr-2 text-xs text-ink-secondary">{platform === "macos" ? "⌘F" : "Ctrl+F"}</span>}
          </div>
          <span className="sr-only" role="status">{query.trim() && (matched.empty ? "没有找到匹配的设置" : `找到 ${matched.items.size} 项设置`)}</span>
          <div className="ml-auto flex shrink-0 items-center gap-3">
            <p id={`${title}-save`} className={`flex items-center gap-1 whitespace-nowrap text-xs ${moving ? "text-accent" : "text-ink-secondary"}`}>
              {moving ? "正在移动仓库，完成或取消后才能关闭" : savedFlash ? <><CheckIcon size={16} aria-hidden className="text-synced" />已保存</> : "更改会自动保存"}
            </p>
            <button type="button" aria-label="关闭偏好设置" title="关闭偏好设置" disabled={moving} onClick={requestClose}
              className="interactive rounded-control p-2 hover:bg-hover active:scale-98 disabled:opacity-50"><XIcon size={16} aria-hidden /></button>
          </div>
        </div>
        <PreferencesNav prefix={title} active={active} matched={matched.sections} statuses={statuses} update={update}
          onJump={id => jumpTo(id, { focus: true, smooth: true })} />
        {reading && <span className="sr-only" role="status">正在读取设置</span>}
        <div ref={content} aria-busy={reading} className="flex min-h-0 flex-col gap-12 overflow-y-auto px-6 pt-6 pb-12">
          <InlineError message={loadError} />
          <PrefSection {...page("appearance")}>
            <SettingRow item="appearance-mode" label={<span id={`${title}-appearance`}>界面外观</span>}>
              {loading ? <ControlSkeleton shape="segmented" /> : <SegmentedControl labelledBy={`${title}-appearance`} options={appearanceOptions}
                value={settings?.appearance} disabled={!!busy || !settings} onChange={value => void change("appearance", async () => setSettings(await ipc.setAppearance(value)))} />}
            </SettingRow>
          </PrefSection>
          <PrefSection {...page("startup")}>
            <SettingRow item="autostart" label={<span id={`${title}-autostart`}>开机时自动启动</span>}>
              {autostart === null && !autoError ? <ControlSkeleton shape="toggle" /> : <Switch aria-labelledby={`${title}-autostart`}
                aria-describedby={autoError ? `${title}-auto-error` : undefined} checked={autostart === true} disabled={!!busy || autostart === null}
                onClick={() => void change("autostart", toggleAutostart)} />}
            </SettingRow>
            <InlineError id={`${title}-auto-error`} message={autoError} />
          </PrefSection>
          <PrefSection {...page("ball")}>
            <SettingRow item="ball-visible" label={<span id={`${title}-ball`}>显示悬浮球</span>}>
              {loading ? <ControlSkeleton shape="toggle" /> : <Switch aria-labelledby={`${title}-ball`} checked={settings?.ballVisible ?? false}
                disabled={!!busy || !settings} onClick={() => void change("ball", async () => setSettings(await ipc.setBallVisible(!settings!.ballVisible)))} />}
            </SettingRow>
            <SettingRow item="ball-autohide" dim={!settings?.ballVisible} label={<span id={`${title}-hide`}>静止后收到屏幕边缘</span>}>
              {loading ? <ControlSkeleton shape="segmented" /> : <SegmentedControl labelledBy={`${title}-hide`} options={autoHideOptions}
                value={settings?.ballAutoHideSeconds ?? "never"} disabled={!!busy || !settings?.ballVisible}
                onChange={value => void change("auto-hide", async () => setSettings(await ipc.setBallAutoHide(value === "never" ? null : value)))} />}
            </SettingRow>
            <BallSizeSetting id={`${title}-ball-size`} ball={ball} loading={loading} visible={!!settings?.ballVisible} disabled={!!busy} />
          </PrefSection>
          <PrefSection {...page("panel")}><SettingRow item="panel-shortcut" stacked>
            <ShortcutRecorder active={matched.sections.has("panel")} onRecordingChange={value => { shortcutRecording.current = value; }} />
          </SettingRow></PrefSection>
          <PrefSection {...page("sync")}><SettingRow item="sync" stacked>
            <SyncPreferences status={sync} readError={syncReadError} error={sectionErrors.sync} setError={message => setError("sync", message)} />
          </SettingRow></PrefSection>
          <PrefSection {...page("library")}>
            <LibrarySettings location={location} move={move} busy={!!busy} exporting={exporting} error={sectionErrors.library} onError={libraryError}
              onMove={restore => void moveLibrary(restore)} onOpen={() => void change("directory", ipc.openLibraryDirectory)} />
          </PrefSection>
          <PrefSection {...page("export")}><SettingRow item="export" stacked>
            <p className="text-xs text-ink-secondary">把全部表情按分类导出成图片文件。最近删除里的表情，以及配文、收藏和使用次数不会导出。</p>
            <div className="flex flex-wrap gap-2">{(["folder", "zip"] as const).map(format =>
              <Button key={format} disabled={!!busy || !overview.data?.all || exporting || move?.running} onClick={() => void startExport("library", format)}>
                {format === "folder" ? "导出到文件夹…" : "导出为 .zip…"}
              </Button>)}</div>
            <ExportStatusContent key={exports.status?.batch ?? 0} inline />
          </SettingRow></PrefSection>
          <PrefSection {...page("update")}>
            <UpdateSettings prefix={title} update={update} settings={settings} loading={loading} busy={!!busy} checkError={checkError ?? updateReadError ?? null} updateReadFailed={!!updateReadError}
              onShow={onShowUpdate} onCheck={() => { setCheckError(null); void ipc.checkUpdate().catch(e => setCheckError(String(e))); }}
              onAuto={() => void change("auto-update", async () => setSettings(await ipc.setAutoCheckUpdates(!settings!.autoCheckUpdates)))} />
          </PrefSection>
          {matched.empty && <div className="m-auto flex max-w-full flex-col items-center gap-3 text-center">
            <MagnifyingGlassIcon size={48} aria-hidden className="text-ink-secondary" /><p className="break-all text-base">没有找到和“{query}”有关的设置</p>
            <p className="text-sm text-ink-secondary">换个关键词，或<button type="button" className="rounded-inset underline underline-offset-4" onClick={clearAndFocus}>清空搜索</button></p>
          </div>}
        </div>
      </div>
    </PreferencesFilterContext.Provider>
  </dialog>;
}
