import { create } from "zustand";

import type { LibraryView } from "../lib/ipc";

interface UiState {
  view: LibraryView;
  selectedStickerId: string | null;
  captionDrafts: Record<string, string>;
  setCaptionDraft: (id: string, value: string | null) => void;
  setView: (view: LibraryView) => void;
  selectSticker: (id: string) => void;
}

export const useUiStore = create<UiState>()((set) => ({
  view: "all",
  selectedStickerId: null,
  captionDrafts: {},
  setCaptionDraft: (id, value) => set((state) => {
    const captionDrafts = { ...state.captionDrafts };
    if (value === null) delete captionDrafts[id];
    else captionDrafts[id] = value;
    return { captionDrafts };
  }),
  setView: (view) => set({ view, selectedStickerId: null }),
  selectSticker: (id) => set({ selectedStickerId: id }),
}));
