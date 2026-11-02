import { useEffect, useRef, useState, type ReactNode } from "react";
import { open, confirm } from "@tauri-apps/plugin-dialog";
import { useQueryClient } from "@tanstack/react-query";
import { Switch } from "../../components/Switch";
import { Button } from "../../components/Button";
import { ipc, type SyncStatus, type SyncBackend, type NetworkTarget, type SyncTargetInfo, type SyncCredentials } from "../../lib/ipc";
import { progressLabel, syncStateLabel, syncTime, waitingNote } from "./useSyncStatus";

const backends = { folder: "同步文件夹", webdav: "WebDAV", r2: "Cloudflare R2", s3: "S3 兼容" };
const inputStyle = "min-w-0 w-full rounded-control border border-line bg-surface px-3 py-2 text-sm disabled:opacity-50";

export function SyncPreferences({ status, focus }: { status?: SyncStatus; focus: boolean }) {
  const section = useRef<HTMLElement>(null);
  const [name, setName] = useState(status?.deviceName ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [reenter, setReenter] = useState(false);
  const [wide, setWide] = useState(false);
  const [backend, setBackend] = useState<SyncBackend>("folder");
  const [url, setUrl] = useState("");
  const [endpoint, setEndpoint] = useState("");
  const [region, setRegion] = useState("");
  const [accountId, setAccountId] = useState("");
  const [bucket, setBucket] = useState("");
  const [path, setPath] = useState("");
  const [credentials, setCredentials] = useState<SyncCredentials>({ username: "", password: "", accessKeyId: "", secretAccessKey: "" });
  const connection = useRef<string | null>(null);
  const working = useRef(false);
  const editingName = useRef(false);
  const mounted = useRef(true);
  const client = useQueryClient();
  useEffect(() => { if (!editingName.current) setName(status?.deviceName ?? ""); }, [status?.deviceName]);
  useEffect(() => {
    if (focus) requestAnimationFrame(() => { section.current?.scrollIntoView({ block: "start" }); section.current?.focus({ preventScroll: true }); });
  }, [focus]);
  useEffect(() => {
    const observer = new ResizeObserver(([entry]) => setWide(entry.contentRect.width >= 420));
    if (section.current) observer.observe(section.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; if (connection.current) void ipc.cancelSyncConnection(connection.current); };
  }, []);
  async function run(action: () => Promise<unknown>) {
    if (working.current) return;
    working.current = true; setBusy(true); setError(null);
    try { await action(); }
    catch (reason) { if (mounted.current) setError(String(reason)); }
    finally { await client.invalidateQueries({ queryKey: ["sync"] }); working.current = false; if (mounted.current) { setBusy(false); setConnecting(false); } }
  }
  async function approve(info: SyncTargetInfo) {
    return !info.manifest || await confirm(
      `这个目标里已经有 ${info.devices} 台设备同步的 ${info.stickers} 张表情。加入后会下载全部表情，并和本机的表情合并。`,
      { title: "加入同步仓库", kind: "info", okLabel: "加入", cancelLabel: "取消" },
    );
  }
  async function choose() {
    const path = await open({ directory: true, multiple: false, title: "选择同步文件夹" });
    if (!path || !mounted.current) return;
    const info = await ipc.inspectSyncFolder(path);
    if (mounted.current && await approve(info) && mounted.current) await ipc.joinSyncFolder(info.path, info.manifest?.id ?? null);
  }
  async function connect() {
    const id = crypto.randomUUID();
    connection.current = id; setConnecting(true);
    try {
      if (reenter) {
        await ipc.updateSyncCredentials(id, credentials);
        if (mounted.current && connection.current === id) setReenter(false);
      } else {
        const target: NetworkTarget = backend === "webdav" ? { backend, url } : backend === "r2" ? { backend, accountId, bucket, path } : { backend: "s3", endpoint, region, bucket, path };
        const info = await ipc.inspectNetworkTarget(id, target, credentials);
        if (!mounted.current || connection.current !== id || !await approve(info)) return;
        if (!mounted.current || connection.current !== id) return;
        await ipc.joinNetworkTarget(id, info.manifest?.id ?? null);
      }
    } catch (reason) {
      if (connection.current === id) throw reason;
    } finally {
      await ipc.cancelSyncConnection(id);
      if (connection.current === id) connection.current = null;
    }
  }
  async function cancel() {
    const id = connection.current;
    connection.current = null;
    if (id) await ipc.cancelSyncConnection(id);
  }
  async function stop() {
    if (await confirm(`停止后，本机的表情和存储后端里的内容都会保留。${status?.target?.credentialsSaved ? "已保存的凭据会从本机钥匙串删除。" : ""}`, { title: "停止同步？", kind: "warning", okLabel: "停止同步", cancelLabel: "取消" })) {
      try { await ipc.stopSync(); } finally { setReenter(false); }
    }
  }
  function secret(field: keyof SyncCredentials, value: string) { setCredentials((current) => ({ ...current, [field]: value })); }
  const target = status?.target;
  const activeBackend = reenter ? target?.backend : backend;
  const address = target?.address ?? "";
  const split = Math.max(0, address.length - 28);
  const progress = progressLabel(status);
  return <section ref={section} tabIndex={-1} aria-labelledby="sync-preferences-title" className="flex scroll-mt-6 flex-col gap-3 rounded-control">
    <h3 id="sync-preferences-title" className="text-xs font-semibold text-ink-secondary">同步</h3>
    {!target && <>
      {wide ? <div className="flex flex-col gap-2">
        <span className="text-xs text-ink-secondary">存储后端</span>
        <div role="group" aria-label="存储后端" className="flex rounded-control bg-muted p-1">
          {Object.entries(backends).map(([value, label]) => <button type="button" key={value} aria-pressed={backend === value} disabled={busy || !status}
            onClick={() => { setBackend(value as SyncBackend); setError(null); }}
            className={`interactive min-w-0 flex-1 whitespace-nowrap rounded-control px-2 py-2 text-xs ${backend === value ? "bg-surface font-semibold" : "text-ink-secondary"}`}>{label}</button>)}
        </div>
      </div> : <label className="flex items-center justify-between gap-4"><span>存储后端</span>
        <select aria-label="存储后端" value={backend} disabled={busy || !status} onChange={(event) => { setBackend(event.target.value as SyncBackend); setError(null); }} className="rounded-control border border-line bg-surface px-3 py-2 text-sm disabled:opacity-50">
          {Object.entries(backends).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
        </select>
      </label>}
      {backend === "folder" && <>
        <p className="text-xs text-ink-secondary">选一个放在 iCloud 云盘、Dropbox 等同步盘里的文件夹，其他电脑选同一个文件夹即可。</p>
        <div><Button disabled={busy || !status} onClick={() => void run(choose)}>选择同步文件夹…</Button></div>
      </>}
    </>}
    {target && <>
      <span className="text-xs text-ink-secondary">{target.name}</span>
      <p title={address} tabIndex={0} className="flex min-w-0 select-text rounded-sm text-xs"
        onCopy={(event) => { event.clipboardData.setData("text/plain", address); event.preventDefault(); }}>
        <span className="min-w-0 truncate">{address.slice(0, split)}</span><span className="shrink-0">{address.slice(split)}</span>
      </p>
      {target.credentialsSaved && <p className="text-xs text-ink-secondary">凭据已保存到系统钥匙串</p>}
      <form className="flex items-center gap-2" onSubmit={(event) => { event.preventDefault(); void run(async () => { await ipc.setDeviceName(name); editingName.current = false; setName(name.trim()); }); }}>
        <label htmlFor="sync-device-name" className="shrink-0">设备名</label>
        <input id="sync-device-name" value={name} disabled={busy} onChange={(event) => { editingName.current = true; setName(event.target.value); }} className="min-w-0 flex-1 rounded-control border border-line bg-surface px-3 py-1" />
        <Button type="submit" disabled={busy || !name.trim() || name.trim() === status?.deviceName}>保存</Button>
      </form>
      <p role="status" className={`text-xs ${status?.phase === "synced" ? "text-synced" : "text-ink-secondary"}`}>
        {progress ?? syncStateLabel(status)}{status?.lastSyncAt ? ` · 上次同步 ${syncTime(status.lastSyncAt)}` : ""}
      </p>
      {status?.progress?.total != null && status.phase === "syncing" && <progress aria-label={progress ?? "同步进度"} max={Math.max(1, status.progress.total)} value={status.progress.completed} className="h-1 w-full accent-accent" />}
      {target.backend !== "folder" && <label className="flex items-center justify-between gap-4 text-xs"><span>使用按流量计费的网络时暂停同步</span><Switch aria-label="使用按流量计费的网络时暂停同步" checked={status?.pauseOnMetered ?? true} disabled={busy} onClick={() => void run(() => ipc.setPauseOnMetered(!(status?.pauseOnMetered ?? true)))} /></label>}
      {waitingNote(status) && <p className="text-xs text-ink-secondary">{waitingNote(status)}</p>}
      {status?.error && <p role="alert" className="text-xs text-accent">{status.error}</p>}
      <div className="flex flex-wrap gap-2">
        <Button disabled={busy} onClick={() => void run(status?.manualPaused ? () => ipc.setSyncPaused(false) : ipc.syncNow)}>{status?.manualPaused ? "继续同步" : "立即同步"}</Button>
        {!status?.manualPaused && <Button disabled={busy} onClick={() => void run(() => ipc.setSyncPaused(true))}>暂停同步</Button>}
        {target.backend !== "folder" && !reenter && <Button disabled={busy} onClick={() => { setReenter(true); setError(null); }}>重新输入密码</Button>}
        <Button disabled={busy} onClick={() => void run(stop)}>停止同步</Button>
      </div>
    </>}
    {((!target && backend !== "folder") || reenter) && <form onSubmit={(event) => { event.preventDefault(); void run(connect); }} className="flex flex-col gap-3">
      <fieldset disabled={busy} className="flex min-w-0 flex-col gap-3">
        {!reenter && <>
          {backend === "webdav" && <Field label="WebDAV 地址"><input type="url" required value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://dav.example.com/表情" className={inputStyle} /></Field>}
          {backend === "webdav" && url.trim().toLowerCase().startsWith("http://") && <p className="text-xs text-accent">密码会以明文传输</p>}
          {backend === "r2" && <Field label="Account ID"><input required value={accountId} onChange={(e) => setAccountId(e.target.value)} autoCapitalize="none" spellCheck={false} className={inputStyle} /></Field>}
          {backend === "s3" && <>
            <Field label="Endpoint"><input type="url" required value={endpoint} onChange={(e) => setEndpoint(e.target.value)} placeholder="https://s3.example.com" className={inputStyle} /></Field>
            <Field label="Region"><input required value={region} onChange={(e) => setRegion(e.target.value)} autoCapitalize="none" spellCheck={false} className={inputStyle} /></Field>
          </>}
          {(backend === "s3" || backend === "r2") && <>
            <Field label="Bucket"><input required value={bucket} onChange={(e) => setBucket(e.target.value)} autoCapitalize="none" spellCheck={false} className={inputStyle} /></Field>
            <Field label="路径（可选）"><input value={path} onChange={(e) => setPath(e.target.value)} autoCapitalize="none" spellCheck={false} className={inputStyle} /></Field>
          </>}
        </>}
        {activeBackend === "webdav" ? <>
          <Field label="用户名"><input required value={credentials.username} onChange={(e) => secret("username", e.target.value)} autoComplete="username" autoCapitalize="none" spellCheck={false} className={inputStyle} /></Field>
          <Field label="密码"><input type="password" required value={credentials.password} onChange={(e) => secret("password", e.target.value)} autoComplete="current-password" className={inputStyle} /></Field>
        </> : <>
          <Field label="Access Key ID"><input required value={credentials.accessKeyId} onChange={(e) => secret("accessKeyId", e.target.value)} autoComplete="off" autoCapitalize="none" spellCheck={false} className={inputStyle} /></Field>
          <Field label="Secret Access Key"><input type="password" required value={credentials.secretAccessKey} onChange={(e) => secret("secretAccessKey", e.target.value)} autoComplete="off" className={inputStyle} /></Field>
        </>}
      </fieldset>
      <div className="flex gap-2">
        <Button type="submit" variant="primary" disabled={busy || !status}>{connecting ? "正在连接…" : "连接"}</Button>
        {connecting && <Button onClick={() => void cancel()}>取消</Button>}
        {reenter && !connecting && <Button disabled={busy} onClick={() => { setReenter(false); setError(null); }}>取消</Button>}
      </div>
    </form>}
    {error && <p role="alert" className="text-xs text-accent">{error}</p>}
  </section>;
}
function Field({ label, children }: { label: string; children: ReactNode }) {
  return <label className="flex min-w-0 flex-col gap-2"><span className="text-xs text-ink-secondary">{label}</span>{children}</label>;
}
