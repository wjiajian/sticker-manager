// 顺序与 core 的 TAG_COLORS 及设计规范一致。
export const TAG_COLORS = [
  { value: "#F2B705", name: "黄" },
  { value: "#8A8A8A", name: "灰" },
  { value: "#E5484D", name: "红" },
  { value: "#3E7BFA", name: "蓝" },
  { value: "#2BA471", name: "绿" },
  { value: "#7A5AF8", name: "紫" },
  { value: "#E8572A", name: "橙" },
  { value: "#A0662F", name: "棕" },
] as const;

export function normalizeTagName(name: string) {
  return name.trim().replace(/[A-Z]/g, (letter) => letter.toLowerCase());
}
