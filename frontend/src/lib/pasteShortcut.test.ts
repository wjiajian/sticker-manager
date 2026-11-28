import { test } from "node:test";
import assert from "node:assert/strict";
import { isPasteShortcut, isEditableTarget, pasteShortcutLabel } from "./pasteShortcut.ts";
const base = { key: "v", code: "KeyV", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false };
test("paste matches the platform modifier and keyboard layout", () => {
  assert.ok(isPasteShortcut(base, "macos"));
  assert.ok(isPasteShortcut({ ...base, key: "V" }, "macos"));
  assert.ok(isPasteShortcut({ ...base, metaKey: false, ctrlKey: true }, "windows"));
  assert.ok(isPasteShortcut({ ...base, key: "м" }, "macos"));
  assert.ok(isPasteShortcut({ ...base, code: "Period" }, "macos"));
  for (const extra of [{ ctrlKey: true }, { altKey: true }, { shiftKey: true }, { key: "x" }]) {
    assert.equal(isPasteShortcut({ ...base, ...extra }, "macos"), false);
  }
  assert.equal(isPasteShortcut(base, "windows"), false);
  assert.equal(pasteShortcutLabel("macos"), "⌘V");
  assert.equal(pasteShortcutLabel("windows"), "Ctrl+V");
});
test("editable targets are excluded including menu and dialog descendants", () => {
  assert.equal(isEditableTarget(null), false);
  assert.equal(isEditableTarget(new EventTarget()), false);
  assert.equal(isEditableTarget({ closest: () => null } as unknown as EventTarget), false);
  assert.equal(
    isEditableTarget({
      isContentEditable: false,
      closest: (selector: string) =>
        selector.includes("[contenteditable]:not([contenteditable='false'])") ? null : {},
    } as unknown as EventTarget),
    false,
  );
  for (const tag of ["input", "textarea", "select", "[role='menu']", "dialog", "[role='textbox']", "[contenteditable]"]) {
    assert.equal(isEditableTarget({ closest: (selector: string) => selector.includes(tag.replace("]", "")) ? {} : null } as unknown as EventTarget), true);
  }
});
