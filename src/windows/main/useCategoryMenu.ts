import type { Resource } from "@tauri-apps/api/core";
import { LogicalPosition } from "@tauri-apps/api/dpi";
import { Image } from "@tauri-apps/api/image";
import { IconMenuItem, Menu, type MenuOptions } from "@tauri-apps/api/menu";
import { confirm } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { ipc, type CategorySummary } from "../../lib/ipc";
import { useLibraryMutation } from "../../lib/queries";
import { CATEGORY_COLORS } from "../../lib/categoryColors";
import { useUiStore } from "../../stores/ui";

export type RenameTarget = { id: string; name: string };

export function useCategoryMenu(onRename: (target: RenameTarget) => void) {
  const resources = useRef<Resource[]>([]);
  const opening = useRef(false);
  const working = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const mutation = useLibraryMutation((operation: () => Promise<unknown>) => operation());

  async function dispose() {
    await Promise.allSettled(resources.current.splice(0).reverse().map((resource) => resource.close()));
  }
  useEffect(() => () => { void dispose(); }, []);

  async function change(operation: () => Promise<unknown>) {
    if (working.current) return;
    working.current = true;
    setError(null);
    try {
      await mutation.mutateAsync(operation);
    } catch (reason) {
      setError(String(reason));
    } finally {
      working.current = false;
    }
  }

  async function popup(makeItems: () => Promise<MenuOptions["items"]>, x: number, y: number) {
    if (opening.current || working.current) return;
    opening.current = true;
    setError(null);
    try {
      await dispose();
      const menu = await Menu.new({ items: await makeItems() });
      resources.current.push(menu);
      // popup 在菜单出现时就返回，资源保留到下次打开菜单或组件卸载。
      await menu.popup(new LogicalPosition(x, y));
    } catch (reason) {
      await dispose();
      setError(`无法打开菜单：${String(reason)}`);
    } finally {
      opening.current = false;
    }
  }

  function categoryMenu(category: CategorySummary, x: number, y: number) {
    return popup(async () => {
      const colors: IconMenuItem[] = [];
      for (const color of CATEGORY_COLORS) {
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = 16;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("无法生成分类色点");
        context.fillStyle = color.value;
        context.beginPath();
        context.arc(8, 8, 4, 0, Math.PI * 2);
        context.fill();
        const icon = await Image.new(new Uint8Array(context.getImageData(0, 0, 16, 16).data.buffer), 16, 16);
        try {
          const item = await IconMenuItem.new({
            id: `category-color-${color.name}`,
            // IconMenuItem 没有 checked 属性，勾选标记放在文字中。
            text: `${color.name}${category.color === color.value ? " ✓" : ""}`,
            icon,
            action: () => { void change(() => ipc.setCategoryColor(category.id, color.value)); },
          });
          colors.push(item);
          resources.current.push(item);
        } finally {
          await icon.close();
        }
      }
      return [
        { id: "rename-category", text: "改名", action: () => onRename({ id: category.id, name: category.name }) },
        { text: "改颜色", items: colors },
        { item: "Separator" },
        { id: "delete-category", text: "删除分类", action: () => { void change(async () => {
          const deletedCount = category.totalStickerCount - category.stickerCount;
          const accepted = await confirm(
            `这个分类下的 ${category.totalStickerCount} 张表情会移到“未分类”${deletedCount > 0 ? `（包含最近删除中的 ${deletedCount} 张）` : ""}，表情本身不会删除。`,
            { title: `删除分类“${category.name}”`, kind: "warning", okLabel: "删除分类", cancelLabel: "取消" },
          );
          if (!accepted) return;
          await ipc.deleteCategory(category.id);
          const { view, setView } = useUiStore.getState();
          if (typeof view !== "string" && view.category === category.id) setView("all");
        }); } },
      ];
    }, x, y);
  }

  return { categoryMenu, error, busy: mutation.isPending };
}
