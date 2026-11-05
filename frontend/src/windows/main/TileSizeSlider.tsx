import { GridNineIcon, SquaresFourIcon } from "@phosphor-icons/react";
import { useEffect } from "react";
import { ipc } from "../../lib/ipc";
import { STICKER_TILE_SIZE, useUiStore } from "../../stores/ui";

export function TileSizeSlider({ onError }: { onError: (error: string | null) => void }) {
  const size = useUiStore((s) => s.stickerTileSize);
  const setSize = useUiStore((s) => s.setStickerTileSize);
  useEffect(() => {
    void ipc.getSettings().then((settings) => setSize(settings.stickerTileSize)).catch((reason: unknown) => onError(String(reason)));
  }, [setSize, onError]);

  function change(value: number) {
    setSize(value);
    onError(null);
    ipc.setStickerTileSize(value).catch((reason: unknown) => onError(String(reason)));
  }

  return (
    <div title="表情大小" className="flex items-center gap-2 text-ink-tertiary">
      <GridNineIcon size={16} aria-hidden />
      <input
        type="range"
        aria-label="表情大小"
        aria-valuetext={`${size} 像素`}
        min={STICKER_TILE_SIZE.min}
        max={STICKER_TILE_SIZE.max}
        step={STICKER_TILE_SIZE.step}
        value={size}
        onChange={(event) => change(Number(event.target.value))}
        className="tile-slider interactive w-20 hover:opacity-82"
      />
      <SquaresFourIcon size={16} aria-hidden />
    </div>
  );
}
