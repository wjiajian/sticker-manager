import { create } from "zustand";

import type { LibraryView } from "../lib/ipc";

interface UiState {
  view: LibraryView;
  selectedStickerId: string | null;
  setView: (view: LibraryView) => void;
  selectSticker: (id: string) => void;
}

export const useUiStore = create<UiState>()((set) => ({
  view: "all",
  selectedStickerId: null,
  setView: (view) => set({ view, selectedStickerId: null }),
  selectSticker: (id) => set({ selectedStickerId: id }),
}));
