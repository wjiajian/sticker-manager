import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "../../lib/ipc";
import { platform } from "../../lib/platform";

export function usePreferences() {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    function keydown(event: KeyboardEvent) {
      if (event.isComposing || event.keyCode === 229 || event.repeat || event.altKey || event.shiftKey) return;
      if (event.key === "," && (platform === "macos" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey)) {
        if (document.querySelector("dialog[open]")) return;
        event.preventDefault(); setOpen(true);
      }
    }
    window.addEventListener("keydown", keydown);
    if (!isTauri()) return () => window.removeEventListener("keydown", keydown);
    let disposed = false;
    const listening = listen("open-preferences", () => {
      if (!disposed) setOpen(true);
      void ipc.takePreferencesRequest().catch((e: unknown) => { if (!disposed) setError(String(e)); });
    });
    void listening.then(() => ipc.takePreferencesRequest()).then((pending) => {
      if (!disposed && pending) setOpen(true);
    }).catch((e: unknown) => { if (!disposed) setError(String(e)); });
    return () => {
      disposed = true;
      window.removeEventListener("keydown", keydown);
      void listening.then((cleanup) => cleanup(), () => {});
    };
  }, []);
  return { open, setOpen, error };
}
