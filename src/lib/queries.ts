import {
  keepPreviousData, QueryClient, skipToken, useInfiniteQuery, useMutation, useQuery, useQueryClient, type InfiniteData,
} from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { ipc, type LibraryView, type StickerCursor, type StickerPage } from "./ipc";

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

const STICKER_PAGE_SIZE = 48;
const stickerPagesKey = ["library", "stickers", "pages"];

/** 按添加时间倒序分页读取全部表情，滚动到底再读下一页。仓库变动时已读的各页依次重读。 */
export function useStickerPages() {
  return useInfiniteQuery({
    queryKey: stickerPagesKey,
    queryFn: ({ pageParam }) => ipc.listStickersPage(pageParam, STICKER_PAGE_SIZE),
    initialPageParam: null as StickerCursor | null,
    getNextPageParam: (page) => page.next,
  });
}

/** 只保留第一页：下次打开从头显示，滚动加载过的页不一直留在内存里。 */
export function keepFirstStickerPage(client: QueryClient) {
  client.setQueryData<InfiniteData<StickerPage, StickerCursor | null>>(stickerPagesKey, (data) =>
    data && { pages: data.pages.slice(0, 1), pageParams: data.pageParams.slice(0, 1) });
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
  return useMutation({
    mutationFn: ipc.importStickers,

  });
}
