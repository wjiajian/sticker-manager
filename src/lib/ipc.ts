import { invoke } from "@tauri-apps/api/core";

// 类型与 crates/core 里的同名结构体一一对应，字段名是 camelCase

export interface LibraryOverview {
  all: number;
  recent: number;
  favorites: number;
  uncategorized: number;
  recentlyDeleted: number;
}

export type FixedLibraryView = "all" | "recent" | "favorites" | "uncategorized" | "recentlyDeleted";
export type LibraryView = FixedLibraryView | { category: string };

export interface Category {
  id: string;
  name: string;
  color: string;
  position: number;
}

export interface CategorySummary extends Category {
  stickerCount: number;
  totalStickerCount: number;
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
  purgeAt: number | null;
  usageCount: number;
  usageDevices: number;
  lastUsedAt: number | null;
  caption: string | null;
  category: Category | null;
  objectPath: string;
  thumbnailPath: string;
}

/** 分页位置：上一页最后一张表情的添加时间和 id。 */
export interface StickerCursor {
  addedAt: number;
  id: string;
}

export interface StickerPage {
  stickers: Sticker[];
  next: StickerCursor | null;
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
  stickerIds: string[];
  cancelled: boolean;
}

export interface ImportStatus { batch: number; running: boolean; progress: { processed: number; total: number }; report: ImportReport | null; error: string | null }
export interface RemoteDevice { id: string; name: string; platform: string; lastSyncAt: number; received: Record<string, number> }
export interface StickerSyncInfo { waitingUpload: boolean; deviceCount: number | null }

export interface PreviewSticker {
  name: string;
  usageCount: number;
  category: string | null;
  thumbnailPath: string;
  objectPath: string;
}
export interface PreviewRequest {
  requestId: number;
  sticker: PreviewSticker;
  fade: boolean;
}

export const ipc = {
  showPreview: (sticker: PreviewSticker, anchor: { top: number; height: number }) => invoke<void>("show_preview", { sticker, anchor }),
  hidePreview: () => invoke<void>("hide_preview"),
  previewSession: () => invoke<PreviewRequest | null>("preview_session"),
  previewReady: (requestId: number, cardHeight: number) => invoke<boolean>("preview_ready", { requestId, cardHeight }),
  importStatus: () => invoke<ImportStatus>("import_status"),
  cancelImport: () => invoke<void>("cancel_import"),
  setStickersCategory: (ids: string[], categoryId: string | null) => invoke<number>("set_stickers_category", { ids, categoryId }),
  createCategoryAndSetStickers: (ids: string[], name: string) => invoke<[Category, number]>("create_category_and_set_stickers", { ids, name }),
  setSyncPaused: (paused: boolean) => invoke<void>("set_sync_paused", { paused }),
  setPauseOnMetered: (enabled: boolean) => invoke<void>("set_pause_on_metered", { enabled }),
  stickerSyncInfo: (id: string) => invoke<StickerSyncInfo | null>("sticker_sync_info", { id }),
  showSyncDetails: () => invoke<void>("show_sync_details"),
  takeSyncDetailsRequest: () => invoke<boolean>("take_sync_details_request"),
  syncStatus: () => invoke<SyncStatus>("sync_status"),
  syncNow: () => invoke<void>("sync_now"),
  inspectSyncFolder: (path: string) => invoke<SyncFolderInfo>("inspect_sync_folder", { path }),
  joinSyncFolder: (path: string, expectedId: string | null) => invoke<void>("join_sync_folder", { path, expectedId }),
  stopSync: () => invoke<void>("stop_sync"),
  inspectNetworkTarget: (connectionId: string, target: NetworkTarget, credentials: SyncCredentials) => invoke<SyncTargetInfo>("inspect_network_target", { connectionId, target, credentials }),
  joinNetworkTarget: (connectionId: string, expectedId: string | null) => invoke<void>("join_network_target", { connectionId, expectedId }),
  cancelSyncConnection: (connectionId: string) => invoke<void>("cancel_sync_connection", { connectionId }),
  updateSyncCredentials: (connectionId: string, credentials: SyncCredentials) => invoke<void>("update_sync_credentials", { connectionId, credentials }),
  setDeviceName: (name: string) => invoke<void>("set_device_name", { name }),
  getSettings: () => invoke<Settings>("get_settings"),
  setAppearance: (appearance: Settings["appearance"]) => invoke<Settings>("set_appearance", { appearance }),
  setBallVisible: (visible: boolean) => invoke<Settings>("set_ball_visible", { visible }),
  setBallAutoHide: (seconds: 3 | 10 | null) => invoke<Settings>("set_ball_auto_hide", { seconds }),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<{ enabled: boolean | null; error: string | null }>("set_autostart", { enabled }),
  libraryDirectory: () => invoke<string>("library_directory"),
  openLibraryDirectory: () => invoke<void>("open_library_directory"),
  takePreferencesRequest: () => invoke<boolean>("take_preferences_request"),
  updateStatus: () => invoke<UpdateStatus>("update_status"),
  checkUpdate: () => invoke<UpdateStatus>("check_update"),
  installUpdate: () => invoke<void>("install_update"),
  ignoreUpdate: (version: string) => invoke<void>("ignore_update", { version }),
  setAutoCheckUpdates: (enabled: boolean) => invoke<Settings>("set_auto_check_updates", { enabled }),
  takeUpdateRequest: () => invoke<boolean>("take_update_request"),
  ballState: () => invoke<BallSnapshot>("ball_state"),
  ballPointerDown: () => invoke<void>("ball_pointer_down"),
  setBallDrop: (paths: string[] | null) => invoke<void>("set_ball_drop", { paths }),
  ballMessage: (message: string) => invoke<void>("ball_message", { message }),
  showBallMenu: () => invoke<void>("show_ball_menu"),
  panelSession: () => invoke<{ pinned: boolean; visible: boolean }>("panel_session"),
  setPanelPinned: (pinned: boolean) => invoke<void>("set_panel_pinned", { pinned }),
  setPanelDragging: (dragging: boolean) => invoke<void>("set_panel_dragging", { dragging }),
  hidePanel: (onlyIfUnpinned = false) => invoke<boolean>("hide_panel", { onlyIfUnpinned }),
  showMainWindow: () => invoke<void>("show_main_window"),
  globalShortcutError: () => invoke<string | null>("global_shortcut_error"),
  libraryOverview: () => invoke<LibraryOverview>("library_overview"),
  listStickers: (view: LibraryView) => invoke<Sticker[]>("list_stickers", { view }),
  listStickersPage: (after: StickerCursor | null, limit: number) => invoke<StickerPage>("list_stickers_page", { after, limit }),
  searchStickers: (query: string) => invoke<Sticker[]>("search_stickers", { query }),
  categories: () => invoke<CategorySummary[]>("categories"),
  sticker: (id: string) => invoke<Sticker | null>("sticker", { id }),
  deleteSticker: (id: string) => invoke<void>("delete_sticker", { id }),
  restoreSticker: (id: string) => invoke<void>("restore_sticker", { id }),
  deleteStickers: (ids: string[]) => invoke<string[]>("delete_stickers", { ids }),
  restoreStickers: (ids: string[]) => invoke<string[]>("restore_stickers", { ids }),
  purgeSticker: (id: string) => invoke<void>("purge_sticker", { id }),
  purgeDeletedStickers: (ids: string[]) => invoke<number>("purge_deleted_stickers", { ids }),
  createCategory: (name: string, color: string | null = null) => invoke<Category>("create_category", { name, color }),
  renameCategory: (id: string, name: string) => invoke<void>("rename_category", { id, name }),
  setCategoryColor: (id: string, color: string) => invoke<void>("set_category_color", { id, color }),
  deleteCategory: (id: string) => invoke<void>("delete_category", { id }),
  setStickerCategory: (stickerId: string, categoryId: string | null) => invoke<void>("set_sticker_category", { stickerId, categoryId }),
  setStickerName: (id: string, name: string) => invoke<void>("set_sticker_name", { id, name }),
  setStickerCaption: (id: string, caption: string) => invoke<void>("set_sticker_caption", { id, caption }),
  importStickers: (paths: string[]) => invoke<ImportReport>("import_stickers", { paths }),
  copySticker: (id: string, fileOnly: boolean) => invoke<void>("copy_sticker", { id, fileOnly }),
  prepareStickerDrag: (id: string) => invoke<{ path: string; thumbnailPath: string }>("prepare_sticker_drag", { id }),
  recordStickerUsage: (id: string) => invoke<void>("record_sticker_usage", { id }),
  toggleStickerFavorite: (id: string) => invoke<boolean>("toggle_sticker_favorite", { id }),
  revealSticker: (id: string) => invoke<void>("reveal_sticker", { id }),
};

export interface Settings {
  version: number;
  appearance: "system" | "light" | "dark";
  ballVisible: boolean;
  ballAutoHideSeconds: 3 | 10 | null;
  ballPosition: { display: string; edge: "left" | "right"; y: number };
  autostartInitialized: boolean;
  sync: (({ backend: "folder"; folder: string } | NetworkTarget) & { libraryId: string; paused: boolean; pauseOnMetered: boolean }) | null;
  autoCheckUpdates: boolean;
  ignoredUpdate: string | null;
}
export interface UpdateStatus {
  currentVersion: string;
  phase: "idle" | "checking" | "downloading" | "installing";
  downloaded: number;
  total: number | null;
  /** 最近一次成功检查的时间（毫秒） */
  lastChecked: number | null;
  available: { version: string; notes: string | null; date: number | null } | null;
  /** 可用版本被忽略过：提示条不显示，偏好设置照常显示 */
  ignored: boolean;
  error: string | null;
}
export interface SyncStatus {
  phase: "unconfigured" | "syncing" | "synced" | "paused" | "error";
  pauseReason: "manual" | "metered" | null;
  manualPaused: boolean;
  pauseOnMetered: boolean;
  devices: RemoteDevice[];
  deviceId: string;
  unseen: number;
  lastSyncAt: number | null;
  error: string | null;
  target: { backend: SyncBackend; name: string; address: string; shortAddress: string; credentialsSaved: boolean } | null;
  progress: { stage: "uploading" | "downloading"; completed: number; total: number | null } | null;
  deviceName: string;
  storageBytes: number;
  onlineDevices: number;
  waiting: number;
}
export type SyncBackend = "folder" | "webdav" | "r2" | "s3";
export type NetworkTarget =
  | { backend: "webdav"; url: string }
  | { backend: "r2"; accountId: string; bucket: string; path: string }
  | { backend: "s3"; endpoint: string; region: string; bucket: string; path: string };
export interface SyncCredentials { username: string; password: string; accessKeyId: string; secretAccessKey: string }
export interface SyncFolderInfo extends SyncTargetInfo { path: string }
export interface SyncTargetInfo {
  manifest: { format: number; id: string; createdAt: number } | null;
  devices: number;
  stickers: number;
}
export interface BallSnapshot {
  center: { x: number; y: number };
  edge: "left" | "right";
  collapsed: boolean;
  hovered: boolean;
  pressed: boolean;
  dragging: boolean;
  panelVisible: boolean;
  dropLabel: string | null;
  message: string | null;
  tipAbove: boolean;
}
