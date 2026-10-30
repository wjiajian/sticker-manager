import { invoke } from "@tauri-apps/api/core";

// 类型与 crates/core 里的同名结构体一一对应，字段名是 camelCase

export interface LibraryOverview {
  all: number;
  recent: number;
  favorites: number;
  untagged: number;
  recentlyDeleted: number;
}

export type LibraryView = "all" | "recent" | "favorites" | "untagged" | "recentlyDeleted";

export interface Sticker {
  id: string;
  name: string;
  width: number;
  height: number;
  byteSize: number;
  animated: boolean;
  addedAt: number;
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
  importStickers: (paths: string[]) => invoke<ImportReport>("import_stickers", { paths }),
};
