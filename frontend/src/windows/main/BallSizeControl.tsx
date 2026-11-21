import { useEffect, useRef, useState } from "react";
import { ipc, type Settings } from "../../lib/ipc";
import { ballSizePreview } from "../../lib/ballSizePreview";

const BALL_SIZE = { min: 40, max: 80, step: 4, default: 56 };
const SIZE_KEYS = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"]);
// 所有对话框实例共享顺序，旧实例的保存/清理不能晚于新实例的预览执行。
const dispatcher = ballSizePreview(ipc.previewBallSize);

export function BallSizeControl({ settings, busy, onError }: {
  settings: Settings | null; busy: boolean; onError: (error: string | null) => void;
}) {
  const [draftBallSize, setDraftBallSize] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const draft = useRef<number | null>(null);
  const pointerDragging = useRef(false);
  const committing = useRef(false);
  const mounted = useRef(false);
  const removePointerListeners = useRef(() => {});
  function reportError(reason: unknown) {
    if (mounted.current) onError(String(reason));
  }
  const savedSize = Math.max(BALL_SIZE.min, Math.min(BALL_SIZE.max, settings?.ballSize ?? BALL_SIZE.default));
  const displayBallSize = draftBallSize ?? savedSize;
  const disabled = busy || saving || !settings?.ballVisible;

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      pointerDragging.current = false;
      removePointerListeners.current();
      // null 会替换待发预览，等待在途预览完成后再还原；已提交的保存继续完成。
      if (draft.current !== null) void dispatcher.preview(null, reportError);
    };
  }, []);

  function preview(size: number) {
    draft.current = size;
    setDraftBallSize(size);
    void dispatcher.preview(size, reportError);
  }
  async function commitBallSize(size = draft.current) {
    if (size === null || committing.current || !mounted.current) return;
    committing.current = true;
    setSaving(true);
    onError(null);
    try {
      await dispatcher.commit(async () => {
        // 前一个对话框的保存可能刚完成，用后端当前值判断是否需要落盘。
        const current = await ipc.getSettings();
        if (size !== current.ballSize) await ipc.setBallSize(size);
      });
      // 整份设置只由 settings-changed 更新，避免旧 IPC 响应覆盖其他控件的新设置。
    } catch (reason) {
      if (mounted.current) onError(String(reason));
    } finally {
      committing.current = false;
      draft.current = null;
      if (mounted.current) { setDraftBallSize(null); setSaving(false); }
    }
  }
  function startPointerDrag() {
    pointerDragging.current = true;
    removePointerListeners.current();
    function finish() {
      removePointerListeners.current();
      pointerDragging.current = false;
      void commitBallSize();
    }
    window.addEventListener("pointerup", finish, { once: true });
    window.addEventListener("pointercancel", finish, { once: true });
    window.addEventListener("blur", finish, { once: true });
    removePointerListeners.current = () => {
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
      window.removeEventListener("blur", finish);
    };
    // 即使按住滑块暂时没改变值，也保持悬浮球展开。
    preview(displayBallSize);
  }

  return <div className={`flex flex-wrap items-center justify-between gap-3 ${settings?.ballVisible ? "" : "text-ink-secondary"}`}>
    <span>悬浮球大小</span>
    <div className="flex items-center gap-2">
      <input type="range" min={BALL_SIZE.min} max={BALL_SIZE.max} step={BALL_SIZE.step}
        value={displayBallSize} disabled={disabled} aria-label="悬浮球大小" aria-valuetext={`${displayBallSize} 像素`}
        className="tile-slider interactive w-32 hover:opacity-82 disabled:opacity-50"
        onPointerDown={(event) => { event.currentTarget.setPointerCapture(event.pointerId); startPointerDrag(); }}
        onLostPointerCapture={() => { if (pointerDragging.current) { removePointerListeners.current(); pointerDragging.current = false; void commitBallSize(); } }}
        onChange={(event) => preview(Number(event.currentTarget.value))}
        onKeyUp={(event) => { if (!pointerDragging.current && SIZE_KEYS.has(event.key)) void commitBallSize(); }}
        onBlur={() => { if (!pointerDragging.current) void commitBallSize(); }} />
      <span className="w-11 text-right text-xs tabular-nums text-ink-secondary">{displayBallSize} px</span>
      <button type="button" disabled={disabled || displayBallSize === BALL_SIZE.default}
        onPointerDown={(event) => event.preventDefault()}
        onClick={() => void commitBallSize(BALL_SIZE.default)}
        className="interactive text-xs underline underline-offset-4 hover:opacity-82 disabled:opacity-50">恢复默认</button>
    </div>
  </div>;
}
