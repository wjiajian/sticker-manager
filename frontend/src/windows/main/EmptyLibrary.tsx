import { ImagesIcon } from "@phosphor-icons/react";
import { platform } from "../../lib/platform";
import { pasteShortcutLabel } from "../../lib/pasteShortcut";
import { Button } from "../../components/Button";
import type { LibraryView, SyncStatus } from "../../lib/ipc";
import { progressLabel, restoringLibrary, syncLabel } from "./useSyncStatus";

export function EmptyLibrary({ view, label, all, sync, busy, onChoose, onSyncSettings, shortcutLabel }: {
  view: LibraryView; label?: string; all?: number; sync?: SyncStatus; busy: boolean;
  onChoose: (directory: boolean) => void; onSyncSettings: () => void; shortcutLabel: string | null;
}) {
  if (view === "all" && all === 0 && restoringLibrary(sync)) return <div role="status" className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
    <h2 className="text-lg font-semibold">正在从同步取回表情</h2>
    <p className="text-ink-secondary">{progressLabel(sync) ?? syncLabel(sync)}</p>
  </div>;

  if (view === "all" && all === 0) return <div className="flex flex-1 flex-col items-center justify-center gap-4 text-center">
    <ImagesIcon size={48} className="text-ink-tertiary" aria-hidden />
    <div className="flex flex-col gap-2">
      <h2 className="text-lg font-semibold">还没有表情</h2>
      <p className="text-sm text-ink-secondary">把图片、文件夹或 .zip 拖进窗口，也可以从这里选择。</p>
      <p className="text-sm text-ink-secondary">也可以按 {pasteShortcutLabel(platform)} 从剪贴板导入。</p>
    </div>
    <div className="flex flex-wrap justify-center gap-2">
      <Button variant="accent" disabled={busy} onClick={() => onChoose(false)} className="disabled:opacity-50">导入表情…</Button>
      <Button disabled={busy} onClick={() => onChoose(true)} className="disabled:opacity-50">导入文件夹…</Button>
    </div>
    {sync && !sync.target && <button type="button" onClick={onSyncSettings} className="rounded-sm text-sm text-ink-secondary underline underline-offset-4 hover:opacity-82">已经在其他电脑上用过？设置同步…</button>}
    {shortcutLabel && <p className="text-xs text-ink-secondary">导入后按 {shortcutLabel} 打开表情面板取用</p>}
  </div>;

  const [title, note] = typeof view === "object" ? [`“${label ?? "分类"}”下还没有表情`, "选中表情后，在详情栏设置为这个分类"]
    : view === "uncategorized" ? all === 0 ? ["还没有表情", "导入的表情会先出现在这里"] : ["所有表情都已分类", null]
    : view === "favorites" ? ["还没有收藏的表情", "在右侧详情栏点星标收藏"]
    : view === "recent" ? ["还没有用过表情", "复制或拖出表情后会出现在这里"]
    : view === "recentlyDeleted" ? ["最近删除里没有表情", "删除的表情会在这里保留 30 天"]
    : ["还没有表情", null];
  return <div className="flex flex-1 flex-col items-center justify-center gap-1 text-center">
    <h2 className="text-base font-semibold">{title}</h2>
    {note && <p className="text-ink-secondary">{note}</p>}
  </div>;
}
