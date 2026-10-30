import {
  ClockCounterClockwiseIcon,
  type Icon,
  SquaresFourIcon,
  StarIcon,
  TrashIcon,
  TrayIcon,
} from "@phosphor-icons/react";
import type { LibraryView } from "../../stores/ui";

export const LIBRARY_VIEWS: { view: LibraryView; label: string; icon: Icon }[] = [
  { view: "all", label: "全部表情", icon: SquaresFourIcon },
  { view: "recent", label: "最近使用", icon: ClockCounterClockwiseIcon },
  { view: "favorites", label: "收藏", icon: StarIcon },
  { view: "untagged", label: "未打标签", icon: TrayIcon },
  { view: "recentlyDeleted", label: "最近删除", icon: TrashIcon },
];
