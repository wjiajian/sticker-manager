import { create } from "zustand";

export type LibraryView = "all" | "recent" | "favorites" | "untagged" | "recentlyDeleted";

interface UiState {
  view: LibraryView;
  setView: (view: LibraryView) => void;
}

export const useUiStore = create<UiState>()((set) => ({
  view: "all",
  setView: (view) => set({ view }),
}));
