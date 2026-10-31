export function TagDot({ color }: { color: string }) {
  return <span aria-hidden className="size-2 shrink-0 rounded-full windows:rounded-xs" style={{ backgroundColor: color }} />;
}
