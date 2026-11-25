import { useEffect, useRef, useState, type ChangeEvent, type RefObject } from "react";
import { ipc, type Settings } from "../../lib/ipc";

type BallSizeAction = { kind: "preview"; size: number | null } | { kind: "commit"; size: number };
export const BALL_SIZE = { min: 40, max: 80, step: 4, default: 56 };

export function useBallSize(settings: Settings | null, settingsRef: RefObject<Settings | null>,
  setSettings: (settings: Settings) => void, setError: (error: string | null) => void) {
  const [draftBallSize, setDraftBallSize] = useState<number | null>(null);
  const draftBallSizeRef = useRef<number | null>(null);
  const pointerDragging = useRef(false);
  const dragCleanup = useRef<(() => void) | null>(null);
  const ballSizeInFlight = useRef<Promise<void> | null>(null);
  const ballSizePending = useRef<BallSizeAction[]>([]);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      dragCleanup.current?.();
      dragCleanup.current = null;
      if (draftBallSizeRef.current !== null || pointerDragging.current) {
        pointerDragging.current = false;
        draftBallSizeRef.current = null;
        enqueueBallSize({ kind: "preview", size: null });
      }
    };
  }, []);
  // 只合并队尾尚未执行的预览；松手后的提交必须保留，后续预览或取消不能覆盖它。
  function enqueueBallSize(action: BallSizeAction) {
    const pending = ballSizePending.current;
    if (pending.at(-1)?.kind === "preview") pending.pop();
    pending.push(action);
    if (ballSizeInFlight.current) return;
    const run = async () => {
      while (pending.length) {
        const next = pending.shift()!;
        try {
          if (next.kind === "preview") {
            await ipc.previewBallSize(next.size);
          } else {
            const current = settingsRef.current;
            if (!current || next.size === current.ballSize) {
              await ipc.previewBallSize(null);
            } else {
              const saved = await ipc.setBallSize(next.size);
              settingsRef.current = saved;
              if (mounted.current) setSettings(saved);
            }
          }
        } catch (reason) {
          if (mounted.current) setError(String(reason));
          if (next.kind === "commit") {
            try {
              const current = await ipc.getSettings();
              settingsRef.current = current;
              if (mounted.current) setSettings(current);
            } catch { /* 保留原始操作错误 */ }
            await ipc.previewBallSize(null).catch(() => {});
          }
        } finally {
          if (next.kind === "commit" && !pending.length && !pointerDragging.current) {
            draftBallSizeRef.current = null;
            if (mounted.current) setDraftBallSize(null);
          }
        }
      }
    };
    const task = run().finally(() => {
      if (ballSizeInFlight.current === task) ballSizeInFlight.current = null;
    });
    ballSizeInFlight.current = task;
  }
  function queueBallSizePreview(value: number | null) {
    enqueueBallSize({ kind: "preview", size: value });
  }
  function commitBallSize(size: number) {
    enqueueBallSize({ kind: "commit", size });
  }
  function cancelBallSizePreview() {
    dragCleanup.current?.();
    dragCleanup.current = null;
    pointerDragging.current = false;
    if (draftBallSizeRef.current !== null || ballSizePending.current.length || ballSizeInFlight.current) {
      draftBallSizeRef.current = null;
      setDraftBallSize(null);
      enqueueBallSize({ kind: "preview", size: null });
    }
  }
  function startBallSizeDrag() {
    dragCleanup.current?.();
    pointerDragging.current = true;
    const finish = () => {
      dragCleanup.current?.();
      dragCleanup.current = null;
      pointerDragging.current = false;
      if (draftBallSizeRef.current !== null) {
        commitBallSize(draftBallSizeRef.current);
      }
    };
    window.addEventListener("pointerup", finish, { once: true });
    window.addEventListener("pointercancel", finish, { once: true });
    dragCleanup.current = () => {
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
    };
  }
  const rawBallSize = Math.min(BALL_SIZE.max, Math.max(BALL_SIZE.min, settings?.ballSize ?? BALL_SIZE.default));
  const savedBallSize = Math.min(
    BALL_SIZE.max,
    BALL_SIZE.min + Math.round((rawBallSize - BALL_SIZE.min) / BALL_SIZE.step) * BALL_SIZE.step,
  );
  const displayBallSize = draftBallSize ?? savedBallSize;
  const commitDraftBeforeClose = () => {
    if (!pointerDragging.current && draftBallSizeRef.current !== null) {
      const finalSize = draftBallSizeRef.current;
      draftBallSizeRef.current = null;
      commitBallSize(finalSize);
    }
  };
  const commitKeyboardDraft = () => {
    if (!pointerDragging.current && draftBallSizeRef.current !== null) {
      commitBallSize(draftBallSizeRef.current);
    }
  };
  return {
    displayBallSize, cancelPreview: cancelBallSizePreview, commitDraftBeforeClose,
    hasPendingWork: () => !!(ballSizePending.current.length || ballSizeInFlight.current),
    resetToDefault: () => {
      draftBallSizeRef.current = BALL_SIZE.default;
      setDraftBallSize(BALL_SIZE.default);
      setError(null);
      commitBallSize(BALL_SIZE.default);
    },
    inputProps: {
      min: BALL_SIZE.min, max: BALL_SIZE.max, step: BALL_SIZE.step, value: displayBallSize,
      "aria-valuetext": `${displayBallSize} 像素`,
      onPointerDown: startBallSizeDrag,
      onChange: (event: ChangeEvent<HTMLInputElement>) => {
        const next = Number(event.target.value);
        draftBallSizeRef.current = next;
        setDraftBallSize(next);
        setError(null);
        queueBallSizePreview(next);
      },
      onKeyUp: commitKeyboardDraft, onBlur: commitKeyboardDraft,
    },
  };
}
