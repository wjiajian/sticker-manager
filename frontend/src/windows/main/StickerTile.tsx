import { CheckIcon } from "@phosphor-icons/react";
import { useRef, type Ref } from "react";
import { CategoryDot } from "../../components/CategoryDot";
import type { Sticker } from "../../lib/ipc";
import { platform } from "../../lib/platform";
import { useUiStore } from "../../stores/ui";
import { StickerImage } from "./StickerImage";
import { useDragOut } from "./useDragOut";
import { usePurgeDays } from "./usePurgeDays";
import type { StickerActions } from "./useStickerActions";

/** 缩略图的边长（crates/core/src/import.rs 的 THUMBNAIL_SIZE）。 */
const THUMBNAIL_SIZE = 256;

/** 点击怎样改变选中：换成这一张、加入或移出多选、从起点连选到这一张。 */
export type SelectMode = "replace" | "toggle" | "range";

export function StickerTile({ sticker, selected, onSelect, onMenu, actions, buttonRef, compact = false, showCategory = false }: {
  sticker: Sticker;
  selected: boolean;
  onSelect: (mode: SelectMode) => void;
  onMenu: (x: number, y: number) => Promise<void>;
  actions: StickerActions;
  buttonRef?: Ref<HTMLButtonElement>;
  compact?: boolean;
  showCategory?: boolean;
}) {
  const drag = useDragOut((isPressed) => actions.drag(sticker, isPressed), sticker.deletedAt !== null || actions.busy);
  const days = usePurgeDays(sticker.purgeAt);
  // 拖出手势会让按钮捕获指针，click 的目标因此总是按钮本身，只能在按下时记下点中的是不是复选框。
  const pressedCheckbox = useRef(false);
  // 格子大到缩略图要被放大时，静态表情换成原图才清晰。动图继续用静止的缩略图，不在网格里播放。
  const thumbnailTooSmall = useUiStore((s) => s.stickerTileSize * window.devicePixelRatio > THUMBNAIL_SIZE);
  const useOriginal = thumbnailTooSmall && !compact && !sticker.animated;
  const categoryName = showCategory && days === null ? sticker.category?.name ?? "未分类" : null;
  return (
    <button
      ref={buttonRef}
      id={compact ? undefined : `sticker-${sticker.id}`}
      type="button"
      aria-label={sticker.name}
      aria-description={categoryName ?? undefined}
      aria-pressed={selected}
      aria-haspopup="menu"
      onPointerDownCapture={(event) => {
        pressedCheckbox.current = event.target instanceof Element && event.target.closest("[data-checkbox]") !== null;
      }}
      onClick={(event) => {
        const toggle = platform === "macos" ? event.metaKey : event.ctrlKey;
        // 复选框只是按钮里的装饰，点中它等同于 ⌘/Ctrl 点击；键盘触发的 click（detail 为 0）不算。
        const checkbox = event.detail > 0 && pressedCheckbox.current;
        onSelect(event.shiftKey ? "range" : toggle || checkbox ? "toggle" : "replace");
      }}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
        // 点在多选里的表情上，菜单针对整个多选。
        if (!selected) onSelect("replace");
        event.currentTarget.focus();
        void onMenu(event.clientX, event.clientY);
      }}
      onKeyDown={(event) => {
        if (event.key !== "ContextMenu" && !(event.key === "F10" && event.shiftKey)) return;
        event.preventDefault();
        if (event.repeat || event.nativeEvent.isComposing) return;
        if (!selected) onSelect("replace");
        const rect = event.currentTarget.getBoundingClientRect();
        void onMenu(rect.left, rect.bottom);
      }}
      {...drag}
      className={`group interactive relative flex touch-none items-center justify-center bg-tile hover:-translate-y-0.5 active:scale-98 ${
        compact ? "size-16 shrink-0 p-[3px]" : "aspect-square min-h-0 w-full p-[5px]"
      } ${selected ? "ring-2 ring-accent ring-offset-2 ring-offset-surface" : ""}`}
    >
      <StickerImage path={useOriginal ? sticker.objectPath : sticker.thumbnailPath} name={sticker.name} lazy />
      {!compact && <>
        <span
          data-checkbox
          aria-hidden="true"
          className={`absolute top-2 left-2 flex size-5 items-center justify-center rounded-sm border-2 windows:rounded-xs ${
            selected ? "border-accent bg-accent-fill text-on-accent" : "border-ink-secondary bg-surface opacity-0 group-hover:opacity-100"
          }`}
        >
          {selected && <CheckIcon size={12} weight="bold" />}
        </span>
        {days !== null ? <span className="absolute bottom-2 left-1/2 max-w-[calc(100%-1rem)] -translate-x-1/2 truncate rounded-sm bg-surface px-2 py-0.5 text-xs font-semibold text-ink-secondary windows:rounded-xs">{days > 0 ? `剩 ${days} 天` : "即将删除"}</span>
          : sticker.caption && <span title={sticker.caption} className={`absolute ${categoryName === null ? "bottom-2" : "bottom-7.5"} left-1/2 max-w-[calc(100%-1rem)] -translate-x-1/2 truncate rounded-sm bg-ink px-2 py-0.5 text-xs font-semibold text-surface windows:rounded-xs`}>{sticker.caption}</span>}
        {/* 固定在格子上，距边和格子内边距相同：方形表情的底边和分类条重合，各格的分类条高度和宽度都一致。 */}
        {categoryName !== null && <span title={categoryName} className="absolute inset-x-[5px] bottom-[5px] flex items-center justify-center gap-1 bg-scrim px-1 py-0.5 text-xs font-semibold text-on-scrim">
          {sticker.category && <CategoryDot color={sticker.category.color} />}
          <span className="truncate">{categoryName}</span>
        </span>}
        {sticker.animated && (
          <span aria-label="动图" className="absolute top-2 right-2 rounded-sm bg-surface px-1 text-xs font-semibold windows:rounded-xs">GIF</span>
        )}
      </>}
    </button>
  );
}
