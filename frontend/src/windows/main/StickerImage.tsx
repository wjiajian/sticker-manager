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
      // 父元素都是带内边距的 flex 容器。h-full 在 WebKit 里会按含内边距的高度
      // （aspect-ratio 算出的）计算，图片因此盖住上下内边距；拉伸不经过百分比。
      className="min-h-0 min-w-0 flex-1 self-stretch object-contain"
    />
  );
}
