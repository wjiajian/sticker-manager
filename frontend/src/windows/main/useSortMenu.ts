import { LogicalPosition } from "@tauri-apps/api/dpi";
import { Menu } from "@tauri-apps/api/menu";
import { useEffect, useRef, useState } from "react";
import { useUiStore, type StickerSort } from "../../stores/ui";

export const STICKER_SORTS: { value: StickerSort; label: string }[] = [
  { value: "added", label: "最近添加" },
  { value: "recent", label: "最近使用" },
  { value: "usage", label: "使用次数" },
  { value: "name", label: "名称" },
];

export function useSortMenu() {
  const menu = useRef<Menu | null>(null);
  const opening = useRef(false);
  const [error, setError] = useState<string | null>(null);

  async function dispose() {
    const previous = menu.current;
    menu.current = null;
    await previous?.close();
  }
  useEffect(() => () => { void dispose(); }, []);

  async function popup(x: number, y: number) {
    if (opening.current) return;
    opening.current = true;
    setError(null);
    try {
      await dispose();
      const { sortOrder, setSortOrder } = useUiStore.getState();
      menu.current = await Menu.new({
        items: STICKER_SORTS.map(({ value, label }) => ({
          id: `sort-${value}`, text: label, checked: sortOrder === value,
          action: () => setSortOrder(value),
        })),
      });
      // popup 在菜单出现时就返回，资源留到下次打开或卸载时再释放。
      await menu.current.popup(new LogicalPosition(x, y));
    } catch (reason) {
      setError(`无法打开排序菜单：${String(reason)}`);
    } finally {
      opening.current = false;
    }
  }

  return { popup, error };
}
