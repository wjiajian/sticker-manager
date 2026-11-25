import { Button } from "../../components/Button";
import { PathSlot } from "../../components/PathSlot";
import { ProgressBar } from "../../components/ProgressBar";
import { Skeleton, useDelayedLoading } from "../../components/Skeleton";
import { ipc, type LibraryLocation, type MoveStatus } from "../../lib/ipc";
import { InlineError, SettingRow } from "./PreferencesLayout";
import { percentage } from "./PreferencesNav";
import { storageSize } from "./useSyncStatus";

export function LibrarySettings({ location, move, busy, exporting, error, onError, onOpen, onMove }: {
  location: LibraryLocation | null; move: MoveStatus | null; busy: boolean; exporting: boolean; error?: string;
  onError: (message: string | null) => void; onOpen: () => void; onMove: (restore: boolean) => void;
}) {
  const loading = !location && !error;
  const show = useDelayedLoading(loading);
  const blocked = busy || move?.running;
  const progress = move?.progress;
  return <>
    <SettingRow item="library-path" stacked label="仓库路径">
      {loading ? <Skeleton label="正在读取仓库位置" className={show ? "" : "invisible"}>
        <div className="h-9 rounded-control bg-muted" /><div className="mt-3 flex gap-2"><div className="h-9 w-24 rounded-control bg-muted" /><div className="h-9 w-24 rounded-control bg-muted" /></div>
      </Skeleton> : location && <>
        <PathSlot value={location.path} label="仓库路径" copyLabel="复制仓库路径" onError={onError} />
        {location.isCustom && <p className="select-text break-all text-xs text-ink-secondary">默认位置：{location.defaultPath}</p>}
        <div><Button disabled={blocked} onClick={onOpen}>打开所在位置</Button></div>
      </>}
    </SettingRow>
    <SettingRow item="library-move" stacked>
      <div className="flex flex-wrap gap-2">
        <Button disabled={blocked || !location?.canMove || exporting} onClick={() => onMove(false)}>移动位置…</Button>
        {location?.isCustom && <Button disabled={blocked || !location.canMove || exporting} onClick={() => onMove(true)}>恢复默认位置</Button>}
      </div>
      <p className="text-xs text-ink-secondary">{location?.cannotMoveReason ?? "请选择本机磁盘上的空文件夹，避免网盘同步文件夹和网络驱动器。移动时会复制全部表情和本机设置，检查无误后才切换，再删除原位置的仓库文件。"}</p>
      {move?.running && <>
        {move.quitRequests > 0 && <InlineError message="正在移动仓库，请等移动完成或取消后再退出。" />}
        <div className="flex flex-wrap items-center gap-3 text-xs">
          {move.stage === "copying" && progress ? <>
            <span role="status">正在复制 {progress.copied}/{progress.total} · {storageSize(progress.bytes)} / {storageSize(progress.totalBytes)}</span>
            <ProgressBar label="移动进度" max={progress.total} value={progress.total ? progress.copied : undefined}
              valueText={progress.total ? percentage(progress.copied, progress.total) : undefined} className="min-w-16 flex-1" />
            <button type="button" className="underline underline-offset-4" onClick={() => void ipc.cancelLibraryMove().catch(e => onError(String(e)))}>取消</button>
          </> : <span role="status">{move.stage === "preparing" ? "正在等同步停下…" : move.stage === "switching" ? "正在切换到新位置…" : "正在清理原位置…"}</span>}
        </div>
      </>}
      {move?.report && <p role="status" className="select-text break-all text-xs">仓库已移到 {move.report.path}
        {!!move.report.leftovers.length && <><br />原位置还有 {move.report.leftovers.length} 项没能删除，可以手动删除：{move.report.previous}</>}</p>}
      <InlineError message={move?.error} />
    </SettingRow>
  </>;
}
