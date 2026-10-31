import { invoke } from "@tauri-apps/api/core";

// 类型与 crates/core 里的同名结构体一一对应，字段名是 camelCase

export interface LibraryOverview {
  all: number;
  recent: number;
  favorites: number;
  untagged: number;
  recentlyDeleted: number;
}

export type FixedLibraryView = "all" | "recent" | "favorites" | "untagged" | "recentlyDeleted";
export type LibraryView = FixedLibraryView | { tag: string };

export interface Tag {
  id: string;
  name: string;
  color: string;
  groupId: string | null;
  position: number;
}

export interface TagSummary extends Tag {
  stickerCount: number;
  totalStickerCount: number;
}

export interface TagGroup {
  id: string;
  name: string;
  position: number;
  tags: TagSummary[];
}

export interface TagSidebar {
  groups: TagGroup[];
  ungrouped: TagSummary[];
}

export interface Sticker {
  id: string;
  name: string;
  width: number;
  height: number;
  byteSize: number;
  animated: boolean;
  addedAt: number;
  favorite: boolean;
  deletedAt: number | null;
  usageCount: number;
  lastUsedAt: number | null;
  caption: string | null;
  tags: Tag[];
  objectPath: string;
  thumbnailPath: string;
}

export interface ImportFailure {
  path: string;
  message: string;
}

export interface ImportReport {
  added: number;
  existing: number;
  restored: number;
  skipped: number;
  failures: ImportFailure[];
}

export const ipc = {
  libraryOverview: () => invoke<LibraryOverview>("library_overview"),
  listStickers: (view: LibraryView) => invoke<Sticker[]>("list_stickers", { view }),
  searchStickers: (query: string) => invoke<Sticker[]>("search_stickers", { query }),
  tagSidebar: () => invoke<TagSidebar>("tag_sidebar"),
  sticker: (id: string) => invoke<Sticker>("sticker", { id }),
  createTag: (name: string, color: string | null = null, groupId: string | null = null) => invoke<Tag>("create_tag", { name, color, groupId }),
  renameTag: (id: string, name: string) => invoke<void>("rename_tag", { id, name }),
  setTagColor: (id: string, color: string) => invoke<void>("set_tag_color", { id, color }),
  moveTag: (id: string, groupId: string | null) => invoke<void>("move_tag", { id, groupId }),
  deleteTag: (id: string) => invoke<void>("delete_tag", { id }),
  createTagGroup: (name: string) => invoke<TagGroup>("create_tag_group", { name }),
  renameTagGroup: (id: string, name: string) => invoke<void>("rename_tag_group", { id, name }),
  deleteTagGroup: (id: string) => invoke<void>("delete_tag_group", { id }),
  addStickerTag: (stickerId: string, tagId: string) => invoke<void>("add_sticker_tag", { stickerId, tagId }),
  removeStickerTag: (stickerId: string, tagId: string) => invoke<void>("remove_sticker_tag", { stickerId, tagId }),
  createTagAndAdd: (stickerId: string, name: string) => invoke<Tag>("create_tag_and_add", { stickerId, name }),
  setStickerCaption: (id: string, caption: string) => invoke<void>("set_sticker_caption", { id, caption }),
  importStickers: (paths: string[]) => invoke<ImportReport>("import_stickers", { paths }),
  copySticker: (id: string, fileOnly: boolean) => invoke<void>("copy_sticker", { id, fileOnly }),
  prepareStickerDrag: (id: string) => invoke<{ path: string; thumbnailPath: string }>("prepare_sticker_drag", { id }),
  recordStickerUsage: (id: string) => invoke<void>("record_sticker_usage", { id }),
  toggleStickerFavorite: (id: string) => invoke<boolean>("toggle_sticker_favorite", { id }),
  revealSticker: (id: string) => invoke<void>("reveal_sticker", { id }),
};
