import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";

export function useStatusEvent<T>(event: string, initial: () => Promise<T>, receive: (value: T) => void, error: (reason: string) => void) {
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let seen = false;
    const listening = listen<T>(event, ({ payload }) => {
      seen = true;
      if (!disposed) receive(payload);
    });
    void listening.then(initial).then(value => { if (!seen && !disposed) receive(value); })
      .catch(reason => { if (!disposed) error(String(reason)); });
    return () => { disposed = true; void listening.then(unlisten => unlisten(), () => {}); };
  }, [event, initial, receive, error]);
}
