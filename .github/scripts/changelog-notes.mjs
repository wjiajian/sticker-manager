// macOS 和 Windows 的更新清单共用：更新说明取 CHANGELOG.md 里该版本的小节，没有就取“未发布”，都没有为空。
export function changelogNotes(changelog, version) {
  return section(changelog, version) ?? section(changelog, "未发布") ?? "";
}

function section(changelog, title) {
  const lines = changelog.split("\n");
  const start = lines.findIndex((line) => line.trim() === `## ${title}`);
  if (start < 0) return null;
  const end = lines.findIndex((line, index) => index > start && line.startsWith("## "));
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n").trim() || null;
}
