import { create } from "zustand";

import type { LibraryView } from "../lib/ipc";

export type StickerTypeFilter = "all" | "static" | "animated";
export type StickerSort = "added" | "recent" | "usage" | "name";

interface UiState {
  view: LibraryView;
  typeFilter: StickerTypeFilter;
  sortOrder: StickerSort;
  setTypeFilter: (filter: StickerTypeFilter) => void;
  setSortOrder: (sort: StickerSort) => void;
  searchText: string;
  searchQuery: string;
  /** 搜索框里按了 ↓ 或 Enter，等当前搜索词的结果回来后选中第一张。 */
  resultFocusRequested: boolean;
  setSearchText: (text: string) => void;
  setSearchQuery: (query: string) => void;
  requestResultFocus: (requested: boolean) => void;
  selectedStickerId: string | null;
  captionDrafts: Record<string, string>;
  setCaptionDraft: (id: string, value: string | null) => void;
  setView: (view: LibraryView) => void;
  selectSticker: (id: string | null) => void;
}

export const useUiStore = create<UiState>()((set) => ({
  view: "all",
  typeFilter: "all",
  sortOrder: "added",
  setTypeFilter: (typeFilter) => set({ typeFilter }),
  setSortOrder: (sortOrder) => set({ sortOrder }),
  searchText: "",
  searchQuery: "",
  resultFocusRequested: false,
  // 继续打字说明还没选好，取消待选中第一张的请求。
  setSearchText: (searchText) => set({ searchText, resultFocusRequested: false }),
  // 只有真的退出搜索才取消选中；没进入搜索时清空搜索框（比如取消输入法组字）不影响选中。
  setSearchQuery: (searchQuery) => set((state) =>
    searchQuery || !state.searchQuery ? { searchQuery } : { searchQuery, selectedStickerId: null }),
  requestResultFocus: (resultFocusRequested) => set({ resultFocusRequested }),
  selectedStickerId: null,
  captionDrafts: {},
  setCaptionDraft: (id, value) => set((state) => {
    const captionDrafts = { ...state.captionDrafts };
    if (value === null) delete captionDrafts[id];
    else captionDrafts[id] = value;
    return { captionDrafts };
  }),
  setView: (view) => set({ view, searchText: "", searchQuery: "", resultFocusRequested: false, selectedStickerId: null }),
  selectSticker: (id) => set({ selectedStickerId: id }),
}));
