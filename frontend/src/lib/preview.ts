import type { PreviewSticker, Sticker } from "./ipc";

export function previewSticker(sticker: Sticker): PreviewSticker {
  return {
    name: sticker.name,
    usageCount: sticker.usageCount,
    category: sticker.category?.name ?? null,
    thumbnailPath: sticker.thumbnailPath,
    objectPath: sticker.objectPath,
  };
}

export function previewDescription(sticker: PreviewSticker) {
  const usage = sticker.usageCount === 0 ? "还没用过" : `用过 ${sticker.usageCount} 次`;
  return sticker.category ? `${usage} · ${sticker.category}` : usage;
}
