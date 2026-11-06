import {
  ClockCounterClockwiseIcon,
  type Icon,
  SquaresFourIcon,
  StarIcon,
  TrashIcon,
  TrayIcon,
} from "@phosphor-icons/react";
import type { FixedLibraryView } from "../../lib/ipc";

const navigationRow = "interactive relative flex w-full min-w-0 items-center gap-2 rounded-control py-1 text-left hover:bg-hover active:scale-98";
/** 侧栏的行样式。面板分类筛选的列表窄，用左右内边距小一档的版本。 */
export const navigation = `${navigationRow} px-2`;
export const compactNavigation = `${navigationRow} px-1`;
export const selectedNavigation = "bg-surface font-semibold windows:before:absolute windows:before:top-2 windows:before:left-0 windows:before:h-3 windows:before:w-0.75 windows:before:rounded-full windows:before:bg-accent";

export const LIBRARY_VIEWS: { view: FixedLibraryView; label: string; icon: Icon }[] = [
  { view: "all", label: "全部表情", icon: SquaresFourIcon },
  { view: "recent", label: "最近使用", icon: ClockCounterClockwiseIcon },
  { view: "favorites", label: "收藏", icon: StarIcon },
  { view: "uncategorized", label: "未分类", icon: TrayIcon },
  { view: "recentlyDeleted", label: "最近删除", icon: TrashIcon },
];
