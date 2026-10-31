import type { Resource } from "@tauri-apps/api/core";
import { LogicalPosition } from "@tauri-apps/api/dpi";
import { Image } from "@tauri-apps/api/image";
import { IconMenuItem, Menu, type MenuOptions } from "@tauri-apps/api/menu";
import { confirm } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { ipc, type TagGroup, type TagSummary } from "../../lib/ipc";
import { useLibraryMutation } from "../../lib/queries";
import { TAG_COLORS } from "../../lib/tagColors";
import { useUiStore } from "../../stores/ui";
import type { TagCreation } from "./TagCreateDialog";

export type RenameTarget = { kind: "tag" | "group"; id: string; name: string };

export function useTagMenu(groups: TagGroup[], onCreate: (creation: TagCreation) => void, onRename: (target: RenameTarget) => void) {
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

  function tagMenu(tag: TagSummary, x: number, y: number) {
    return popup(async () => {
      const colors: IconMenuItem[] = [];
      for (const color of TAG_COLORS) {
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = 16;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("无法生成标签色点");
        context.fillStyle = color.value;
        context.beginPath();
        context.arc(8, 8, 4, 0, Math.PI * 2);
        context.fill();
        const icon = await Image.new(new Uint8Array(context.getImageData(0, 0, 16, 16).data.buffer), 16, 16);
        try {
          const item = await IconMenuItem.new({
            id: `tag-color-${color.name}`,
            // IconMenuItem 没有 checked 属性，勾选标记放在文字中。
            text: `${color.name}${tag.color === color.value ? " ✓" : ""}`,
            icon,
            action: () => { void change(() => ipc.setTagColor(tag.id, color.value)); },
          });
          colors.push(item);
          resources.current.push(item);
        } finally {
          await icon.close();
        }
      }
      return [
        { id: "rename-tag", text: "改名", action: () => onRename({ kind: "tag", id: tag.id, name: tag.name }) },
        { text: "改颜色", items: colors },
        { text: "移到标签分组", items: [
          ...groups.map((group) => ({
            id: `move-tag-${group.id}`, text: group.name, checked: tag.groupId === group.id,
            action: () => { void change(() => ipc.moveTag(tag.id, group.id)); },
          })),
          { id: "move-tag-ungrouped", text: "未分组", checked: tag.groupId === null, action: () => { void change(() => ipc.moveTag(tag.id, null)); } },
        ] },
        { item: "Separator" },
        { id: "delete-tag", text: "删除标签", action: () => { void change(async () => {
          const deletedCount = tag.totalStickerCount - tag.stickerCount;
          const accepted = await confirm(
            `会从 ${tag.totalStickerCount} 张表情中移除这个标签${deletedCount > 0 ? `（包含最近删除中的 ${deletedCount} 张）` : ""}。`,
            { title: `删除标签“${tag.name}”`, kind: "warning", okLabel: "删除标签", cancelLabel: "取消" },
          );
          if (!accepted) return;
          await ipc.deleteTag(tag.id);
          const { view, setView } = useUiStore.getState();
          if (typeof view !== "string" && view.tag === tag.id) setView("all");
        }); } },
      ];
    }, x, y);
  }

  function groupMenu(group: TagGroup | null, x: number, y: number) {
    return popup(async () => {
      const items: MenuOptions["items"] = [
        { id: "create-tag", text: "新建标签", action: () => onCreate({ kind: "tag", groupId: group?.id ?? null }) },
      ];
      if (group) {
        items.push(
          { id: "rename-group", text: "改名", action: () => onRename({ kind: "group", id: group.id, name: group.name }) },
          { item: "Separator" },
          { id: "delete-group", text: "删除标签分组", action: () => { void change(() => ipc.deleteTagGroup(group.id)); } },
        );
      } else {
        items.push({ id: "create-group", text: "新建标签分组", action: () => onCreate({ kind: "group" }) });
      }
      return items;
    }, x, y);
  }

  return { tagMenu, groupMenu, error, busy: mutation.isPending };
}
