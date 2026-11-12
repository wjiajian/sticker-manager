// Windows 使用独立清单，不覆盖 macOS 的 latest.json 或发布流程。
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { changelogNotes } from "./changelog-notes.mjs";

const { VERSION: version, BASE_URL: baseUrl } = process.env;
if (!version || !baseUrl) throw new Error("需要 VERSION 和 BASE_URL 环境变量");
const nsis = "target/release/bundle/nsis";
const signatures = readdirSync(nsis).filter((name) => name.endsWith(".exe.sig"));
if (signatures.length !== 1) throw new Error("应该正好有一个 NSIS 更新签名");
const signature = readFileSync(join(nsis, signatures[0]), "utf8").trim();
if (!signature) throw new Error("NSIS 更新签名为空");
const output = "target/release/bundle/release";
const name = `sticker-manager_${version}_x64-setup.exe`;
// 确认上传的安装包存在；签名对应的是构建后原样复制的同一文件。
readFileSync(join(output, name));
const notes = changelogNotes(readFileSync("CHANGELOG.md", "utf8"), version);
writeFileSync(join(output, "latest-windows.json"), `${JSON.stringify({
  version, notes, pub_date: new Date().toISOString(),
  platforms: { "windows-x86_64": { signature, url: `${baseUrl}/${name}` } },
}, null, 2)}\n`);
writeFileSync("target/release/bundle/release-notes.md", `${notes}\n`);
