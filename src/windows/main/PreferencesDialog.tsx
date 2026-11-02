import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button } from "../../components/Button";
import { Switch } from "../../components/Switch";
import { ipc, type Settings, type SyncStatus } from "../../lib/ipc";
import { platform } from "../../lib/platform";
import { SyncPreferences } from "./SyncPreferences";

export function PreferencesDialog({ onClose, sync, focusSync = false }: { onClose: () => void; sync?: SyncStatus; focusSync?: boolean }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const title = useId();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autoError, setAutoError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [path, setPath] = useState("");
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const working = useRef(false);
  const focusedInitially = useRef(false);
  useEffect(() => {
    const element = dialog.current!;
    element.showModal();
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
  useEffect(() => {
    if (focusedInitially.current || focusSync) return;
    const firstSwitch = dialog.current?.querySelector<HTMLButtonElement>('button[role="switch"]:not(:disabled)');
    if (firstSwitch) { firstSwitch.focus(); focusedInitially.current = true; }
  }, [settings, autostart, focusSync]);

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
  return <dialog ref={dialog} aria-labelledby={title} onCancel={(event) => { event.preventDefault(); onClose(); }}
    className="m-auto max-h-[calc(100%-48px)] w-128 max-w-[calc(100%-48px)] overflow-y-auto rounded-card border border-line bg-surface p-6 text-ink backdrop:bg-overlay">
    <div className="flex flex-col gap-6">
      <h2 id={title} className="text-lg font-semibold">偏好设置</h2>
      <Group title="通用">
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
      <SyncPreferences status={sync} focus={focusSync} />
      <Group title="仓库">
        <span className="text-xs text-ink-secondary">仓库路径</span>
        <p title={path} tabIndex={0} className="flex min-w-0 select-text rounded-sm text-xs"
          onCopy={(event) => { event.clipboardData.setData("text/plain", path); event.preventDefault(); }}>
          <span className="min-w-0 truncate">{path.slice(0, split)}</span><span className="shrink-0">{path.slice(split)}</span>
        </p>
        <div><Button disabled={!!busy || !path} onClick={() => void change("directory", ipc.openLibraryDirectory)}>打开所在位置</Button></div>
      </Group>
      {error && <p role="alert" className="text-xs text-accent">{error}</p>}
      <div className="flex justify-end"><Button variant="primary" onClick={onClose}>完成</Button></div>
    </div>
  </dialog>;
}
function Group({ title, children }: { title: string; children: ReactNode }) {
  return <section className="flex flex-col gap-3"><h3 className="text-xs font-semibold text-ink-secondary">{title}</h3>{children}</section>;
}
