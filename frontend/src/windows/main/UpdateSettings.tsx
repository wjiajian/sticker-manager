import { Button } from "../../components/Button";
import { Switch } from "../../components/Switch";
import type { Settings, UpdateStatus } from "../../lib/ipc";
import { ControlSkeleton, InlineError, SettingRow } from "./PreferencesLayout";
import { downloadLabel, focusOpener } from "./useUpdate";
import { syncTime } from "./useSyncStatus";

export function UpdateSettings({ prefix, update, updateReadFailed, settings, loading, busy, checkError, onCheck, onShow, onAuto }: {
  prefix: string; update?: UpdateStatus; updateReadFailed: boolean; settings: Settings | null; loading: boolean; busy: boolean;
  checkError: string | null; onCheck: () => void; onShow: () => void; onAuto: () => void;
}) {
  return <>
    <SettingRow item="update-check" stacked>
      <div className="flex min-h-9 items-center justify-between gap-4">
        {update ? <span>当前版本 {update.currentVersion}</span> : updateReadFailed ? <span>当前版本</span> : <ControlSkeleton shape="version" />}
        <Button disabled={!update || update.phase !== "idle"} onClick={onCheck}>{update?.phase === "checking" ? "正在检查…" : "检查更新"}</Button>
      </div>
      {update && <UpdateNote status={update} onShow={onShow} />}
      <InlineError message={checkError} />
    </SettingRow>
    <SettingRow item="update-auto" label={<span id={`${prefix}-auto-update`}>自动检查更新</span>} descriptionId={`${prefix}-auto-update-note`}
      description="启动时和每天检查一次，发现新版本只提示，确认后才下载">
      {loading ? <ControlSkeleton shape="toggle" /> : <Switch aria-labelledby={`${prefix}-auto-update`} aria-describedby={`${prefix}-auto-update-note`}
        checked={settings?.autoCheckUpdates ?? false} disabled={busy || !settings} onClick={onAuto} />}
    </SettingRow>
  </>;
}

function UpdateNote({ status, onShow }: { status: UpdateStatus; onShow: () => void }) {
  if (status.phase === "downloading" || status.phase === "installing") return <p role="status" className="text-xs">{downloadLabel(status)}</p>;
  if (status.error) return <InlineError message={status.error} />;
  if (status.available) return <div className="flex items-center justify-between gap-4 text-xs">
    <span role="status">发现新版本 {status.available.version}{status.ignored ? "（已忽略）" : ""}</span>
    <button type="button" onClick={(event) => { focusOpener(event.currentTarget); onShow(); }} className="shrink-0 underline underline-offset-4">查看</button>
  </div>;
  if (status.lastChecked && status.phase === "idle") return <p role="status" className="text-xs text-ink-secondary">已是最新版本 · 检查于 {syncTime(status.lastChecked)}</p>;
  return null;
}
