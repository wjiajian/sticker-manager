import { create } from "zustand";

export type StickerDropTarget = { kind: "category"; id: string } | { kind: "uncategorized" };

interface StickerDragState {
  target: StickerDropTarget | null;
  count: number;
  set(target: StickerDropTarget | null, count: number): void;
  clear(): void;
}

export const useStickerDragStore = create<StickerDragState>((set) => ({
  target: null,
  count: 0,
  set: (target, count) => set({ target, count }),
  clear: () => set({ target: null, count: 0 }),
}));
