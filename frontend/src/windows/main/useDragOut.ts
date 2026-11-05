import { useRef, type MouseEvent, type PointerEvent } from "react";

export function useDragOut(start: (isPressed: () => boolean) => Promise<void>, disabled: boolean) {
  const gesture = useRef({ x: 0, y: 0, pressed: false, started: false });

  return {
    onPointerDown(event: PointerEvent<HTMLElement>) {
      if (disabled || event.button !== 0 || !event.isPrimary) return;
      gesture.current = { x: event.clientX, y: event.clientY, pressed: true, started: false };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    onPointerMove(event: PointerEvent<HTMLElement>) {
      const current = gesture.current;
      if (!(event.buttons & 1)) current.pressed = false;
      if (!current.pressed || current.started || Math.hypot(event.clientX - current.x, event.clientY - current.y) < 6) return;
      current.started = true;
      const element = event.currentTarget;
      const pointerId = event.pointerId;
      void start(() => current.pressed).finally(() => {
        current.pressed = false;
        if (element.hasPointerCapture(pointerId)) element.releasePointerCapture(pointerId);
      });
    },
    onPointerUp() { gesture.current.pressed = false; },
    onPointerCancel() { gesture.current.pressed = false; },
    onClickCapture(event: MouseEvent<HTMLElement>) {
      if (gesture.current.started && event.detail > 0) {
        event.preventDefault();
        event.stopPropagation();
      }
    },
  };
}
