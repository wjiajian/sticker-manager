import { TAG_COLORS } from "../lib/tagColors";

export function TagDot({ color }: { color: string }) {
  const token = TAG_COLORS.find((item) => item.value.toLowerCase() === color.toLowerCase())?.token;
  return <span aria-hidden className="size-2 shrink-0 rounded-full windows:rounded-xs" style={{ backgroundColor: token ? `var(--color-tag-${token})` : color }} />;
}
