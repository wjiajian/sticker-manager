import type { ImportReport } from "./ipc";

export function failureCount(report: ImportReport) {
  return report.failures.reduce((sum, failure) => sum + failure.count, 0);
}

export function formatReport(report: ImportReport) {
  let summary = `${report.cancelled ? "已取消：" : ""}新增 ${report.added} 张，${report.existing} 张已存在`;
  if (report.restored) summary += `（其中恢复 ${report.restored} 张）`;
  if (report.skipped) summary += `，跳过 ${report.skipped} 项`;
  const failed = failureCount(report);
  if (failed) summary += `，${failed} 项导入失败`;
  return summary;
}
