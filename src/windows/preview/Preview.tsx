import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ipc, type PreviewRequest } from "../../lib/ipc";
import { previewDescription } from "../../lib/preview";

type Content = PreviewRequest & { image: string | null };

async function decode(path: string) {
  const image = new Image();
  image.decoding = "async";
  image.src = convertFileSrc(path);
  await image.decode();
  return image.src;
}

export function Preview() {
  const card = useRef<HTMLDivElement>(null);
  const latest = useRef(0);
  const [content, setContent] = useState<Content | null>(null);
  const [shown, setShown] = useState(0);

  useEffect(() => {
    let disposed = false;
    async function receive(request: PreviewRequest) {
      if (disposed || request.requestId <= latest.current) return;
      latest.current = request.requestId;
      const current = () => !disposed && latest.current === request.requestId;
      const thumbnail = await decode(request.sticker.thumbnailPath).catch(() => null);
      if (!current()) return;
      setContent((previous) => ({ ...request, image: previous?.sticker.objectPath === request.sticker.objectPath
        ? previous.image ?? thumbnail : thumbnail }));
      // 解码不占用面板 WebView；旧请求完成后不能把原图换回去。
      const original = await decode(request.sticker.objectPath).catch(() => null);
      if (current() && original) setContent((previous) => previous?.requestId === request.requestId ? { ...previous, image: original } : previous);
    }
    const listeners = [
      listen<PreviewRequest>("preview-content", ({ payload }) => { void receive(payload); }),
      listen<number>("preview-hidden", ({ payload }) => {
        if (payload < latest.current) return;
        latest.current = payload; setContent(null); setShown(0);
      }),
    ];
    void Promise.all(listeners).then(() => ipc.previewSession()).then((request) => { if (request) void receive(request); }).catch(console.error);
    return () => { disposed = true; for (const pending of listeners) void pending.then((unlisten) => unlisten(), () => {}); };
  }, []);

  useLayoutEffect(() => {
    if (!content) return;
    const { requestId } = content;
    let disposed = false;
    let first = 0;
    let second = 0;
    function ready() {
      if (latest.current !== requestId || !card.current) return;
      void ipc.previewReady(requestId, card.current.getBoundingClientRect().height).then((visible) => {
        if (!disposed && visible && latest.current === requestId) setShown(requestId);
      }).catch(console.error);
    }
    // 隐藏的 WKWebView 不运行 rAF：首次显示以 DOM 提交、图片解码为准，
    // 原生窗口出现后才淡入。已显示时等两帧再移动，避免旧内容闪到新位置。
    if (content.fade) ready();
    else first = requestAnimationFrame(() => { second = requestAnimationFrame(ready); });
    return () => { disposed = true; cancelAnimationFrame(first); cancelAnimationFrame(second); };
  }, [content?.requestId]);

  if (!content) return null;
  return <div aria-hidden="true" className="pointer-events-none p-16">
    <div ref={card} className={`flex w-42 flex-col gap-2 rounded-2xl bg-surface p-3 shadow-preview ${content.fade ? "transition-opacity duration-300 ease-standard" : ""} ${content.fade && shown !== content.requestId ? "opacity-0" : "opacity-100"}`}>
      <div className="flex size-36 shrink-0 items-center justify-center rounded-lg bg-tile p-4">
        {content.image ? <img src={content.image} alt="" draggable={false} decoding="async" className="h-full w-full object-contain" />
          : <span className="text-xs text-ink-secondary">图片无法显示</span>}
      </div>
      <p className="truncate text-xs font-semibold">{content.sticker.name}</p>
      <p className="line-clamp-2 text-xs break-words text-ink-secondary">{previewDescription(content.sticker)}</p>
    </div>
  </div>;
}
