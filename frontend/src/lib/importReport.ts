import type { ImportReport } from "./ipc";

export function formatReport(report: ImportReport) {
  let summary = `${report.cancelled ? "已取消：" : ""}新增 ${report.added} 张，${report.existing} 张已存在`;
  if (report.restored) summary += `（其中恢复 ${report.restored} 张）`;
  if (report.skipped) summary += `，跳过 ${report.skipped} 项`;
  if (report.failures.length) summary += `，${report.failures.length} 项导入失败`;
  return summary;
}
