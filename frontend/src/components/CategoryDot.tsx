import { CATEGORY_COLORS } from "../lib/categoryColors";

export function CategoryDot({ color }: { color: string }) {
  const token = CATEGORY_COLORS.find((item) => item.value.toLowerCase() === color.toLowerCase())?.token;
  return <span aria-hidden className="size-2 shrink-0 rounded-full windows:rounded-xs" style={{ backgroundColor: token ? `var(--color-category-${token})` : color }} />;
}
