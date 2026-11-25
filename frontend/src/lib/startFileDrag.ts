import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { ipc } from "./ipc";
import { platform } from "./platform";

/** 发起一次原生文件复制拖拽，返回是否已被某个目标接收。 */
export function startFileDrag(path: string, iconPath: string): Promise<boolean> {
  if (platform === "windows") return ipc.startStickerDrag(path, iconPath);
  return new Promise<boolean>((resolve, reject) => {
    void startDrag({ item: [path], icon: iconPath, mode: "copy" }, (event) => {
      resolve(event.result === "Dropped");
    }).catch(reject);
  });
}
