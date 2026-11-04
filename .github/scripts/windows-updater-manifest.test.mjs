import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const script = fileURLToPath(new URL("./windows-updater-manifest.mjs", import.meta.url));
test("Windows manifest preserves macOS assets and selects release notes", () => {
  const root = mkdtempSync(join(tmpdir(), "sticker-windows-manifest-"));
  try {
    const output = join(root, "target/release/bundle/release");
    const nsis = join(root, "target/release/bundle/nsis");
    mkdirSync(output, { recursive: true });
    mkdirSync(nsis, { recursive: true });
    writeFileSync(join(root, "CHANGELOG.md"), "## 未发布\nfuture\n## 1.2.3\nWindows notes\n## 1.2.2\nold\n");
    writeFileSync(join(output, "latest.json"), "macOS manifest");
    writeFileSync(join(output, "sticker-manager_1.2.3_x64-setup.exe"), "installer");
    const run = () => spawnSync(process.execPath, [script], {
      cwd: root, encoding: "utf8",
      env: { ...process.env, VERSION: "1.2.3", BASE_URL: "https://example.com/v1.2.3" },
    });
    assert.notEqual(run().status, 0, "missing signature must fail");
    writeFileSync(join(nsis, "installer.exe.sig"), "test-signature\n");
    const result = run();
    assert.equal(result.status, 0, result.stderr);
    const manifest = JSON.parse(readFileSync(join(output, "latest-windows.json"), "utf8"));
    assert.equal(manifest.notes, "Windows notes");
    assert.deepEqual(Object.keys(manifest.platforms), ["windows-x86_64"]);
    assert.equal(manifest.platforms["windows-x86_64"].signature, "test-signature");
    assert.equal(manifest.platforms["windows-x86_64"].url, "https://example.com/v1.2.3/sticker-manager_1.2.3_x64-setup.exe");
    assert.equal(readFileSync(join(output, "latest.json"), "utf8"), "macOS manifest");
  } finally {
    // mkdtemp 返回的专用目录；仅清除此测试生成的数据。
    rmSync(root, { recursive: true, force: true });
  }
});
