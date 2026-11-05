import { CopySimpleIcon, MinusIcon, SquareIcon, XIcon } from "@phosphor-icons/react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

const controlClass = "grid w-11.5 place-items-center focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent disabled:opacity-40";

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const available = isTauri();

  useEffect(() => {
    if (!available) return;
    let disposed = false;
    const nativeWindow = getCurrentWindow();
    async function refresh() {
      try {
        const value = await nativeWindow.isMaximized();
        if (!disposed) setMaximized(value);
      } catch (reason) {
        if (!disposed) setError(String(reason));
      }
    }
    void refresh();
    const listening = nativeWindow.onResized(() => { void refresh(); });
    void listening.catch((reason) => { if (!disposed) setError(String(reason)); });
    return () => {
      disposed = true;
      void listening.then((unlisten) => unlisten(), () => {});
    };
  }, [available]);

  async function run(action: () => Promise<void>) {
    setError(null);
    try { await action(); }
    catch (reason) { setError(String(reason)); }
  }

  return <div className="relative -my-3 -mr-4 flex shrink-0 self-stretch" role="group" aria-label="窗口控制">
    <button type="button" aria-label="最小化" title="最小化" disabled={!available} className={`${controlClass} hover:bg-hover`}
      onClick={() => { void run(() => getCurrentWindow().minimize()); }}><MinusIcon size={16} /></button>
    <button type="button" aria-label={maximized ? "还原窗口" : "最大化"} title={maximized ? "还原窗口" : "最大化"} disabled={!available} className={`${controlClass} hover:bg-hover`}
      onClick={() => { void run(() => getCurrentWindow().toggleMaximize()); }}>
      {maximized ? <CopySimpleIcon size={16} /> : <SquareIcon size={16} />}
    </button>
    <button type="button" aria-label="关闭窗口" title="关闭窗口" disabled={!available} className={`${controlClass} hover:bg-[#C42B1C] hover:text-white`}
      onClick={() => { void run(() => getCurrentWindow().close()); }}><XIcon size={16} /></button>
    {error && <p role="alert" className="absolute top-full right-2 z-50 mt-2 w-72 rounded-control border border-line bg-surface p-3 text-sm text-ink">窗口操作失败：{error}</p>}
  </div>;
}
