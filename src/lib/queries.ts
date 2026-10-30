import { QueryClient, useQuery } from "@tanstack/react-query";
import { ipc } from "./ipc";

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
