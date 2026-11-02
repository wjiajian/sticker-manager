import type { PreviewSticker, Sticker, TagSidebar } from "./ipc";

export function previewSticker(sticker: Sticker, sidebar?: TagSidebar): PreviewSticker {
  const order = new Map([...(sidebar?.groups.flatMap((group) => group.tags) ?? []), ...(sidebar?.ungrouped ?? [])]
    .map((tag, index) => [tag.id, index]));
  return {
    name: sticker.name,
    usageCount: sticker.usageCount,
    tags: [...sticker.tags].sort((a, b) => (order.get(a.id) ?? Infinity) - (order.get(b.id) ?? Infinity)).map((tag) => tag.name),
    thumbnailPath: sticker.thumbnailPath,
    objectPath: sticker.objectPath,
  };
}

export function previewDescription(sticker: PreviewSticker) {
  const usage = sticker.usageCount === 0 ? "还没用过" : `用过 ${sticker.usageCount} 次`;
  return sticker.tags.length ? `${usage} · ${sticker.tags.join(" ")}` : usage;
}
