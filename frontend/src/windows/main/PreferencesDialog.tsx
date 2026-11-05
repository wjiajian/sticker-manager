import { ArchiveIcon, ArrowCircleUpIcon, CloudIcon, SlidersHorizontalIcon, SmileyIcon, type Icon } from "@phosphor-icons/react";
import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button } from "../../components/Button";
import { Switch } from "../../components/Switch";
import { ipc, type Settings, type SyncStatus, type UpdateStatus } from "../../lib/ipc";
import { platform } from "../../lib/platform";
import { navigation, selectedNavigation } from "./Sidebar";
import { SyncPreferences } from "./SyncPreferences";
import { downloadLabel, focusOpener } from "./useUpdate";
import { syncTime } from "./useSyncStatus";

type SectionId = "general" | "quick" | "sync" | "library" | "update";
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
  const [path, setPath] = useState("");
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const working = useRef(false);
  const [checkError, setCheckError] = useState<string | null>(null);
  const tabId = (id: SectionId) => `${title}-tab-${id}`;
  useEffect(() => {
    const element = dialog.current!;
    element.showModal();
    // showModal 会把焦点给第一个可聚焦元素，这里换成当前分组的侧边选项。
    document.getElementById(tabId(section))?.focus();
    let disposed = false;
    const listening = listen<Settings>("settings-changed", ({ payload }) => { if (!disposed) setSettings(payload); });
    void listening.then(() => ipc.getSettings()).then((s) => { if (!disposed) setSettings(s); })
      .catch((e: unknown) => { if (!disposed) setError(String(e)); });
    void ipc.getAutostart().then((enabled) => { if (!disposed) setAutostart(enabled); })
      .catch((e: unknown) => { if (!disposed) setAutoError(String(e)); });
    void ipc.libraryDirectory().then((value) => { if (!disposed) setPath(value); })
      .catch((e: unknown) => { if (!disposed) setError(String(e)); });
    void ipc.globalShortcutError().then((value) => { if (!disposed) setShortcutError(value); })
      .catch((e: unknown) => { if (!disposed) setShortcutError(String(e)); });
    return () => { disposed = true; element.close(); void listening.then((cleanup) => cleanup(), () => {}); };
  }, []);

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
      try { setSettings(await ipc.getSettings()); } catch { /* 保留原始操作错误 */ }
    } finally { working.current = false; setBusy(null); }
  }
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
  return <dialog ref={dialog} aria-labelledby={title} onCancel={(event) => { event.preventDefault(); onClose(); }}
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
            </Group>
            <Group title="表情面板">
              <div className="flex items-center justify-between gap-4"><span>呼出快捷键</span><kbd className="rounded-control bg-muted px-3 py-1 font-sans">{platform === "macos" ? "⌃⌥E" : "Ctrl+Alt+E"}</kbd></div>
              <p className="text-xs text-ink-secondary">暂不支持修改</p>
              {shortcutError && <p role="alert" className="text-xs text-accent">{shortcutError}</p>}
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
              <div><Button disabled={!!busy || !path} onClick={() => void change("directory", ipc.openLibraryDirectory)}>打开所在位置</Button></div>
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
          <Button variant="primary" onClick={onClose}>完成</Button>
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
