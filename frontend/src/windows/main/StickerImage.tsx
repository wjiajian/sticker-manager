import { convertFileSrc } from "@tauri-apps/api/core";
import { useState } from "react";

export function StickerImage({ path, name, lazy = false }: { path: string; name: string; lazy?: boolean }) {
  const [failed, setFailed] = useState(false);

  return failed ? (
    <span className="text-xs text-ink-secondary">图片无法显示</span>
  ) : (
    <img
      src={convertFileSrc(path)}
      alt={name}
      draggable={false}
      loading={lazy ? "lazy" : "eager"}
      decoding="async"
      onError={() => setFailed(true)}
      className="h-full w-full object-contain"
    />
  );
}
