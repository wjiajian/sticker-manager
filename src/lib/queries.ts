import { keepPreviousData, QueryClient, skipToken, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { ipc, type LibraryView } from "./ipc";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // 数据只会被本应用的命令或同步改动，改动后显式让对应查询失效
      staleTime: Infinity,
      refetchOnWindowFocus: false,
      // 本地命令失败重试也不会成功
      retry: false,
    },
  },
});

export function useLibraryOverview() {
  return useQuery({ queryKey: ["library", "overview"], queryFn: ipc.libraryOverview });
}

export function useTagSidebar() {
  return useQuery({ queryKey: ["library", "tags"], queryFn: ipc.tagSidebar });
}

export function useLibraryMutation<TData, TVariables>(mutationFn: (variables: TVariables) => Promise<TData>) {
  const client = useQueryClient();
  return useMutation({
    mutationFn,
    scope: { id: "library-metadata" },
    onSettled: () => client.invalidateQueries({ queryKey: ["library"] }),
  });
}

export function useSticker(id: string | null) {
  return useQuery({ queryKey: ["library", "sticker", id], queryFn: id === null ? skipToken : () => ipc.sticker(id) });
}

export function useStickers(view: LibraryView) {
  return useQuery({ queryKey: ["library", "stickers", view], queryFn: () => ipc.listStickers(view) });
}

export function useStickerSearch(query: string) {
  return useQuery({
    queryKey: ["library", "search", query],
    queryFn: query.trim() ? () => ipc.searchStickers(query) : skipToken,
    placeholderData: keepPreviousData,
  });
}

/** 仓库在界面之外被改动（比如到期自动清理）时，Rust 发出 library-changed，所有仓库查询重新读取。 */
export function useLibraryChangedEvents() {
  const client = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    const listening = listen("library-changed", () => { void client.invalidateQueries({ queryKey: ["library"] }); });
    void listening.catch((reason: unknown) => setError(`无法监听仓库更新：${String(reason)}`));
    return () => { void listening.then((unlisten) => unlisten(), () => {}); };
  }, [client]);
  return error;
}

export function useImportStickers() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ipc.importStickers,
    // 连续拖入时按批次导入，避免覆盖尚未完成的请求。
    scope: { id: "import-stickers" },
    onSettled: () => client.invalidateQueries({ queryKey: ["library"] }),
  });
}
