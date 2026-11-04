// 把 tauri build 生成的更新包改成固定的 ASCII 文件名，并生成更新检查读取的 latest.json。
// 用法：VERSION=0.1.2 BASE_URL=https://github.com/<owner>/<repo>/releases/download/v0.1.2 node .github/scripts/updater-manifest.mjs
// 输出到 target/release/bundle/updater/。更新说明取 CHANGELOG.md 里该版本的小节，没有就取“未发布”。
import { copyFileSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const { VERSION: version, BASE_URL: baseUrl } = process.env;
if (!version || !baseUrl) throw new Error("需要 VERSION 和 BASE_URL 环境变量");

const bundle = "target/release/bundle/macos";
const archives = readdirSync(bundle).filter((name) => name.endsWith(".app.tar.gz"));
if (archives.length !== 1) throw new Error(`${bundle} 里应该正好有一个 .app.tar.gz，实际是 ${archives.length} 个`);
const archive = join(bundle, archives[0]);
const signature = readFileSync(`${archive}.sig`, "utf8").trim();

const arch = { arm64: "aarch64", x64: "x86_64" }[process.arch];
if (!arch) throw new Error(`不支持的架构 ${process.arch}`);
const name = `sticker-manager_${version}_${arch}.app.tar.gz`;
const output = "target/release/bundle/updater";
mkdirSync(output, { recursive: true });
copyFileSync(archive, join(output, name));

function section(changelog, title) {
  const lines = changelog.split("\n");
  const start = lines.findIndex((line) => line.trim() === `## ${title}`);
  if (start < 0) return null;
  const end = lines.findIndex((line, index) => index > start && line.startsWith("## "));
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n").trim() || null;
}
const changelog = readFileSync("CHANGELOG.md", "utf8");
const notes = section(changelog, version) ?? section(changelog, "未发布") ?? "";

const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: { [`darwin-${arch}`]: { signature, url: `${baseUrl}/${name}` } },
};
writeFileSync(join(output, "latest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`已生成 ${join(output, name)} 和 latest.json（darwin-${arch}，说明 ${notes.length} 字）`);
