import { CheckIcon, CopyIcon, FolderOpenIcon, StarIcon } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { platform } from "../../lib/platform";
import { useSticker } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";
import { CaptionEditor } from "./CaptionEditor";
import { StickerTags } from "./StickerTags";
import { StickerImage } from "./StickerImage";
import { useDragOut } from "./useDragOut";
import type { StickerActions } from "./useStickerActions";

export function DetailPanel({ actions }: { actions: StickerActions }) {
  const selectedId = useUiStore((s) => s.selectedStickerId);
  const { data: sticker, error, isPending } = useSticker(selectedId);
  const unavailable = !sticker || sticker.deletedAt !== null;
  const drag = useDragOut((isPressed) => sticker ? actions.drag(sticker, isPressed) : Promise.resolve(), unavailable || actions.busy);
  const revealLabel = platform === "windows" ? "在资源管理器中显示" : "在访达中显示";
  const copied = actions.copiedId === sticker?.id;

  return (
    <aside
      aria-label="表情详情"
      className={`flex w-66 shrink-0 flex-col gap-6 overflow-y-auto bg-detail p-6 ${sticker ? "" : "items-center justify-center text-center text-ink-secondary"}`}
    >
      {sticker ? (
        <>
          <div {...drag} className={`relative flex aspect-square w-full touch-none items-center justify-center rounded-card bg-tile p-4 ${unavailable ? "" : "cursor-grab active:cursor-grabbing"}`}>
            <StickerImage key={sticker.id} path={sticker.objectPath} name={sticker.name} />
            {sticker.animated && (
              <span aria-label="动图" className="absolute top-3 right-3 rounded-sm bg-surface px-1 text-xs font-semibold windows:rounded-xs">GIF</span>
            )}
          </div>
          <div className="flex flex-col gap-1">
            <h2 className="text-lg font-semibold break-words">{sticker.name}</h2>
            <p className="text-xs text-ink-secondary">
              {sticker.width} × {sticker.height} · {formatSize(sticker.byteSize)} · {new Date(sticker.addedAt).toLocaleDateString("zh-CN")} 添加
            </p>
          </div>
          <div className="flex gap-2">
            <Button
              variant="accent"
              aria-label={copied ? "已复制" : "复制"}
              disabled={unavailable || actions.busy}
              onClick={() => void actions.copy(sticker)}
              title={platform === "macos" ? "⌘C 复制 · ⌘⇧C 复制为文件" : "Ctrl+C 复制 · Ctrl+Shift+C 复制为文件"}
              className="flex-1 justify-center text-base disabled:opacity-50"
            >
              {copied ? <CheckIcon size={16} aria-hidden /> : <CopyIcon size={16} aria-hidden />}
              <span role="status">{copied ? "已复制" : "复制"}</span>
            </Button>
            <Button aria-label={revealLabel} title={revealLabel} disabled={actions.busy} onClick={() => void actions.reveal(sticker)} className="disabled:opacity-50">
              <FolderOpenIcon size={16} aria-hidden />
            </Button>
            <Button
              aria-label={sticker.favorite ? "取消收藏" : "收藏"}
              title={sticker.favorite ? "取消收藏" : "收藏"}
              aria-pressed={sticker.favorite}
              disabled={unavailable || actions.busy}
              onClick={() => void actions.toggleFavorite(sticker)}
              className="disabled:opacity-50"
            >
              <StarIcon size={16} weight={sticker.favorite ? "fill" : "regular"} className={sticker.favorite ? "text-accent" : ""} aria-hidden />
            </Button>
          </div>
          <StickerTags key={`tags-${sticker.id}`} sticker={sticker} />
          <CaptionEditor key={`caption-${sticker.id}`} sticker={sticker} />
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-xs">
            <dt className="text-ink-secondary">使用次数</dt>
            <dd>{sticker.usageCount.toLocaleString("zh-CN")} 次</dd>
            <dt className="text-ink-secondary">上次使用</dt>
            <dd>{sticker.lastUsedAt === null ? "尚未使用" : new Date(sticker.lastUsedAt).toLocaleString("zh-CN", { year: "numeric", month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" })}</dd>
            {platform === "windows" && <>
              <dt className="text-ink-secondary">快捷键</dt>
              <dd>Ctrl+C 复制<br />Ctrl+Shift+C 复制为文件</dd>
            </>}
          </dl>
        </>
      ) : error ? <p role="alert">{String(error)}</p> : selectedId && isPending ? <p role="status">正在读取表情…</p> : "选中一张表情查看详情"}
      {actions.error && <p role="alert" className="text-xs text-ink-secondary">{actions.error}</p>}
    </aside>
  );
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
