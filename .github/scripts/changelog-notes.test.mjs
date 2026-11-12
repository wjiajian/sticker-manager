import { test } from "node:test";
import assert from "node:assert/strict";
import { changelogNotes } from "./changelog-notes.mjs";

const changelog = "# 更新记录\n\n## 未发布\n\nfuture\n\n## 1.2.3\n\n- notes\n- more\n\n## 1.2.2\n\nold\n";

test("takes the section of the version, up to the next heading", () => {
  assert.equal(changelogNotes(changelog, "1.2.3"), "- notes\n- more");
  assert.equal(changelogNotes(changelog, "1.2.2"), "old");
});

test("falls back to the unreleased section, then to empty", () => {
  assert.equal(changelogNotes(changelog, "9.9.9"), "future");
  assert.equal(changelogNotes("## 1.0.0\n\nx\n", "9.9.9"), "");
});

test("an empty section counts as missing", () => {
  assert.equal(changelogNotes("## 未发布\n\nfuture\n## 1.2.3\n\n## 1.2.2\nold\n", "1.2.3"), "future");
});
