import { isTauri } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import type { ImportReport } from "../../lib/ipc";
import { useImportStickers } from "../../lib/queries";

export function useImport() {
  const { mutate, data, error, isPending } = useImportStickers();
  const [choosing, setChoosing] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [dropError, setDropError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (disposed) return;
      setDragging(payload.type === "enter" || payload.type === "over");
      if (payload.type === "drop" && payload.paths.length > 0) {
        setDialogError(null);
        mutate(payload.paths);
      }
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    }).catch((reason: unknown) => {
      if (!disposed) setDropError(`无法启用拖入导入：${String(reason)}`);
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [mutate]);

  async function choose(directory: boolean) {
    setChoosing(true);
    setDialogError(null);
    try {
      const paths = await open({
        title: directory ? "导入文件夹" : "导入表情",
        multiple: true,
        directory,
        filters: directory ? undefined : [{ name: "表情", extensions: ["png", "jpg", "jpeg", "webp", "gif", "apng"] }],
      });
      if (paths && paths.length > 0) mutate(paths);
    } catch (reason) {
      setDialogError(`无法打开选择框：${String(reason)}`);
    } finally {
      setChoosing(false);
    }
  }

  const result = dialogError ?? (error ? `导入未完成：${String(error)}` : data ? formatReport(data) : "");
  return {
    choose,
    dragging,
    busy: choosing || isPending,
    message: dragging ? "松开以导入表情" : isPending ? "正在导入表情…" : [result, dropError].filter(Boolean).join("\n"),
  };
}

function formatReport(report: ImportReport) {
  let summary = `新增 ${report.added} 张，${report.existing} 张已存在`;
  if (report.restored) summary += `（其中恢复 ${report.restored} 张）`;
  if (report.skipped) summary += `，跳过 ${report.skipped} 项`;
  if (report.failures.length) summary += `，${report.failures.length} 项导入失败`;
  return [summary, ...report.failures.map(({ path, message }) => `${path}：${message}`)].join("\n");
}
