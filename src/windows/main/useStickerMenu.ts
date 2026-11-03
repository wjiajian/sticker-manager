import { LogicalPosition } from "@tauri-apps/api/dpi";
import { Menu, type MenuOptions } from "@tauri-apps/api/menu";
import { useEffect, useRef, useState } from "react";
import type { Sticker } from "../../lib/ipc";
import { platform } from "../../lib/platform";
import type { StickerActions } from "./useStickerActions";

export function useStickerMenu(actions: StickerActions) {
  const menu = useRef<Menu | null>(null);
  const opening = useRef(false);
  const [error, setError] = useState<string | null>(null);

  async function dispose() {
    const previous = menu.current;
    menu.current = null;
    await previous?.close();
  }
  useEffect(() => () => { void dispose(); }, []);

  async function popup(sticker: Sticker, x: number, y: number) {
    if (opening.current || actions.busy) return;
    opening.current = true;
    setError(null);
    try {
      await dispose();
      const items: MenuOptions["items"] = sticker.deletedAt === null ? [
        { id: "copy-sticker", text: "复制", action: () => { void actions.copy(sticker); } },
        ...(platform === "windows" ? [{ id: "copy-sticker-file", text: "复制为文件", action: () => { void actions.copy(sticker, true); } }] : []),
        { id: "reveal-sticker", text: platform === "windows" ? "在资源管理器中显示" : "在访达中显示", action: () => { void actions.reveal(sticker); } },
        { item: "Separator" },
        { id: "delete-sticker", text: "删除", action: () => { void actions.deleteSticker(sticker); } },
      ] : [
        { id: "restore-sticker", text: "恢复", action: () => { void actions.restoreSticker(sticker); } },
        { item: "Separator" },
        { id: "purge-sticker", text: "彻底删除…", action: () => { void actions.purgeSticker(sticker); } },
      ];
      menu.current = await Menu.new({ items });
      // 原生菜单出现时 popup 就返回，留到下次打开或卸载时再释放。
      await menu.current.popup(new LogicalPosition(x, y));
    } catch (reason) {
      setError(`无法打开菜单：${String(reason)}`);
    } finally {
      opening.current = false;
    }
  }

  return { popup, error };
}
