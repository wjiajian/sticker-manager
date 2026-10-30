import { useStickers } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";
import { StickerImage } from "./StickerImage";

export function DetailPanel() {
  const view = useUiStore((s) => s.view);
  const selectedId = useUiStore((s) => s.selectedStickerId);
  const { data: stickers } = useStickers(view);
  const sticker = stickers?.find((item) => item.id === selectedId);

  return (
    <aside
      aria-label="表情详情"
      className={`flex w-66 shrink-0 flex-col gap-6 overflow-y-auto bg-detail p-6 ${sticker ? "" : "items-center justify-center text-center text-ink-secondary"}`}
    >
      {sticker ? (
        <>
          <div className="relative flex aspect-square w-full items-center justify-center rounded-card bg-tile p-4">
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
        </>
      ) : "选中一张表情查看详情"}
    </aside>
  );
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
