import { useEffect, useRef, useState } from "react";
import { open, confirm } from "@tauri-apps/plugin-dialog";
import { useQueryClient } from "@tanstack/react-query";
import { Button } from "../../components/Button";
import { ipc, type SyncStatus } from "../../lib/ipc";
import { syncLabel, syncTime, waitingNote } from "./useSyncStatus";

export function SyncPreferences({ status, focus }: { status?: SyncStatus; focus: boolean }) {
  const section = useRef<HTMLElement>(null);
  const [name, setName] = useState(status?.deviceName ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const working = useRef(false);
  const editingName = useRef(false);
  const client = useQueryClient();
  useEffect(() => { if (!editingName.current) setName(status?.deviceName ?? ""); }, [status?.deviceName]);
  useEffect(() => {
    if (focus) requestAnimationFrame(() => { section.current?.scrollIntoView({ block: "start" }); section.current?.focus({ preventScroll: true }); });
  }, [focus]);
  async function run(action: () => Promise<unknown>) {
    if (working.current) return;
    working.current = true; setBusy(true); setError(null);
    try { await action(); }
    catch (reason) { setError(String(reason)); }
    finally { await client.invalidateQueries({ queryKey: ["sync"] }); working.current = false; setBusy(false); }
  }
  async function choose() {
    const path = await open({ directory: true, multiple: false, title: "选择同步文件夹" });
    if (!path) return;
    const info = await ipc.inspectSyncFolder(path);
    if (info.manifest && !await confirm(
      `这个文件夹里已经有 ${info.devices} 台设备同步的 ${info.stickers} 张表情。加入后会下载全部表情，并和本机的表情合并。`,
      { title: "加入同步仓库", kind: "info", okLabel: "加入", cancelLabel: "取消" },
    )) return;
    await ipc.joinSyncFolder(info.path, info.manifest?.id ?? null);
  }
  async function stop() {
    if (await confirm("停止后，本机的表情和同步文件夹里的内容都会保留。", { title: "停止同步？", kind: "warning", okLabel: "停止同步", cancelLabel: "取消" })) await ipc.stopSync();
  }
  const path = status?.folder ?? "";
  const split = Math.max(0, path.length - 28);
  return <section ref={section} tabIndex={-1} aria-labelledby="sync-preferences-title" className="flex scroll-mt-6 flex-col gap-3 rounded-control">
    <h3 id="sync-preferences-title" className="text-xs font-semibold text-ink-secondary">同步</h3>
    {!path ? <>
      <p className="text-xs text-ink-secondary">选一个放在 iCloud 云盘、Dropbox 等同步盘里的文件夹，其他电脑选同一个文件夹即可。</p>
      <div><Button disabled={busy || !status} onClick={() => void run(choose)}>选择同步文件夹…</Button></div>
    </> : <>
      <span className="text-xs text-ink-secondary">同步文件夹</span>
      <p title={path} tabIndex={0} className="flex min-w-0 select-text rounded-sm text-xs"
        onCopy={(event) => { event.clipboardData.setData("text/plain", path); event.preventDefault(); }}>
        <span className="min-w-0 truncate">{path.slice(0, split)}</span><span className="shrink-0">{path.slice(split)}</span>
      </p>
      <form className="flex items-center gap-2" onSubmit={(event) => { event.preventDefault(); void run(async () => { await ipc.setDeviceName(name); editingName.current = false; setName(name.trim()); }); }}>
        <label htmlFor="sync-device-name" className="shrink-0">设备名</label>
        <input id="sync-device-name" value={name} disabled={busy} onChange={(event) => { editingName.current = true; setName(event.target.value); }}
          className="min-w-0 flex-1 rounded-control border border-line bg-surface px-3 py-1" />
        <Button type="submit" disabled={busy || !name.trim() || name.trim() === status?.deviceName}>保存</Button>
      </form>
      <p role="status" className={`text-xs ${status?.phase === "synced" ? "text-synced" : "text-ink-secondary"}`}>
        {syncLabel(status)}{status?.lastSyncAt ? ` · 上次同步 ${syncTime(status.lastSyncAt)}` : ""}
      </p>
      {waitingNote(status) && <p className="text-xs text-ink-secondary">{waitingNote(status)}</p>}
      {status?.error && <p role="alert" className="text-xs text-accent">{status.error}</p>}
      <div className="flex flex-wrap gap-2">
        <Button disabled={busy} onClick={() => void run(ipc.syncNow)}>立即同步</Button>
        <Button disabled={busy} onClick={() => void run(stop)}>停止同步</Button>
      </div>
    </>}
    {error && <p role="alert" className="text-xs text-accent">{error}</p>}
  </section>;
}
