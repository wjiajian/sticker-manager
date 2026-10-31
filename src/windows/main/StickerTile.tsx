import type { Ref } from "react";
import type { Sticker } from "../../lib/ipc";
import { StickerImage } from "./StickerImage";
import { useDragOut } from "./useDragOut";
import { usePurgeDays } from "./usePurgeDays";
import type { StickerActions } from "./useStickerActions";

export function StickerTile({ sticker, selected, onSelect, onMenu, actions, buttonRef, compact = false }: {
  sticker: Sticker;
  selected: boolean;
  onSelect: () => void;
  onMenu: (x: number, y: number) => Promise<void>;
  actions: StickerActions;
  buttonRef?: Ref<HTMLButtonElement>;
  compact?: boolean;
}) {
  const drag = useDragOut((isPressed) => actions.drag(sticker, isPressed), sticker.deletedAt !== null || actions.busy);
  const days = usePurgeDays(sticker.purgeAt);
  return (
    <button
      ref={buttonRef}
      id={compact ? undefined : `sticker-${sticker.id}`}
      type="button"
      aria-label={sticker.name}
      aria-pressed={selected}
      aria-haspopup="menu"
      onClick={onSelect}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
        onSelect();
        event.currentTarget.focus();
        void onMenu(event.clientX, event.clientY);
      }}
      onKeyDown={(event) => {
        if (event.key !== "ContextMenu" && !(event.key === "F10" && event.shiftKey)) return;
        event.preventDefault();
        if (event.repeat || event.nativeEvent.isComposing) return;
        onSelect();
        const rect = event.currentTarget.getBoundingClientRect();
        void onMenu(rect.left, rect.bottom);
      }}
      {...drag}
      className={`interactive relative flex touch-none items-center justify-center bg-tile hover:-translate-y-0.5 active:scale-98 ${
        compact ? "size-16 shrink-0 rounded-control p-2" : "aspect-square min-h-0 w-full rounded-card p-4"
      } ${selected ? "ring-2 ring-accent ring-offset-2 ring-offset-surface" : ""}`}
    >
      <StickerImage path={sticker.thumbnailPath} name={sticker.name} lazy />
      {!compact && <>
        {days !== null ? <span className="absolute bottom-2 left-1/2 max-w-[calc(100%-1rem)] -translate-x-1/2 truncate rounded-sm bg-surface px-2 py-0.5 text-xs font-semibold text-ink-secondary windows:rounded-xs">{days > 0 ? `剩 ${days} 天` : "即将删除"}</span>
          : sticker.caption && <span title={sticker.caption} className="absolute bottom-2 left-1/2 max-w-[calc(100%-1rem)] -translate-x-1/2 truncate rounded-sm bg-ink px-2 py-0.5 text-xs font-semibold text-surface windows:rounded-xs">{sticker.caption}</span>}
        {sticker.animated && (
          <span aria-label="动图" className="absolute top-2 right-2 rounded-sm bg-surface px-1 text-xs font-semibold windows:rounded-xs">GIF</span>
        )}
      </>}
    </button>
  );
}
