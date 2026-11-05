// 顺序与 core 的 CATEGORY_COLORS 及设计规范一致。
export const CATEGORY_COLORS = [
  { value: "#F2B705", name: "黄", token: "yellow" },
  { value: "#8A8A8A", name: "灰", token: "gray" },
  { value: "#E5484D", name: "红", token: "red" },
  { value: "#3E7BFA", name: "蓝", token: "blue" },
  { value: "#2BA471", name: "绿", token: "green" },
  { value: "#7A5AF8", name: "紫", token: "purple" },
  { value: "#E8572A", name: "橙", token: "orange" },
  { value: "#A0662F", name: "棕", token: "brown" },
] as const;

export function normalizeCategoryName(name: string) {
  return name.trim().replace(/[A-Z]/g, (letter) => letter.toLowerCase());
}
