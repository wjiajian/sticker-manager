import { invoke } from "@tauri-apps/api/core";

// 类型与 crates/core 里的同名结构体一一对应，字段名是 camelCase

export interface LibraryOverview {
  all: number;
  recent: number;
  favorites: number;
  untagged: number;
  recentlyDeleted: number;
}

export const ipc = {
  libraryOverview: () => invoke<LibraryOverview>("library_overview"),
};
