import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { ipc, type PanelShortcutState } from "./ipc";

export const PANEL_SHORTCUT_KEY = ["panel-shortcut"] as const;
export function mergePanelShortcut(client: QueryClient, next: PanelShortcutState) {
  client.setQueryData<PanelShortcutState>(PANEL_SHORTCUT_KEY, (old) => !old || next.revision >= old.revision ? next : old);
}
export function usePanelShortcut() {
  const client = useQueryClient();
  return useQuery({ queryKey: PANEL_SHORTCUT_KEY, enabled: isTauri(), queryFn: async () => {
    const next = await ipc.panelShortcutState();
    const old = client.getQueryData<PanelShortcutState>(PANEL_SHORTCUT_KEY);
    return old && old.revision > next.revision ? old : next;
  } });
}
export function usePanelShortcutEvents(): string | null {
  const client = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const listening = listen<PanelShortcutState>("panel-shortcut-changed", ({ payload }) => {
      if (!disposed) mergePanelShortcut(client, payload);
    });
    // 注册监听后再读一次，补齐首次查询与监听就绪之间的变化。
    void listening.then(() => ipc.panelShortcutState()).then((state) => {
      if (!disposed) mergePanelShortcut(client, state);
    }).catch((reason: unknown) => { if (!disposed) setError(`无法监听快捷键更新：${String(reason)}`); });
    return () => { disposed = true; void listening.then((unlisten) => unlisten(), () => {}); };
  }, [client]);
  return error;
}
