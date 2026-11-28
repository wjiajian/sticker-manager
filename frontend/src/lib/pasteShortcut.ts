import type { Platform } from "./platform";

type PasteKey = Pick<KeyboardEvent, "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;
export function isPasteShortcut(event: PasteKey, platform: Platform): boolean {
  const modifier = platform === "macos" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  const key = event.key.toLowerCase();
  return modifier && !event.altKey && !event.shiftKey && (key === "v" || (!/^[a-z]$/.test(key) && event.code === "KeyV"));
}
export function isEditableTarget(target: EventTarget | null): boolean {
  return !!(target && "closest" in target && typeof target.closest === "function" && target.closest("input, textarea, select, [contenteditable]:not([contenteditable='false']), [role='textbox'], [role='combobox'], [role='listbox'], [role='menu'], dialog, [role='dialog']"));
}
export function pasteShortcutLabel(platform: Platform): string {
  return platform === "macos" ? "⌘V" : "Ctrl+V";
}
