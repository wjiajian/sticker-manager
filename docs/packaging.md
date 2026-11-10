# 打包和发布

macOS（Apple Silicon）和 Windows x64 都能从源码打包。macOS 只有临时签名（ad-hoc），没有 Apple 开发者签名和公证；Windows 安装包没有代码签名。

## macOS 本地打包

```bash
pnpm --dir frontend tauri build --bundles app
CI=true pnpm --dir frontend tauri build --bundles app,dmg
```

产物在 `target/release/bundle/macos/表情管理器.app` 和 `target/release/bundle/dmg/`。

- 只用 `--bundles dmg` 时，Tauri 会清理作为中间产物的 `.app`；两个都要就写 `app,dmg`。
- `CI=true` 跳过 DMG 的访达窗口美化，避免构建时弹出自动化权限提示。
- `tauri.conf.json` 的 `bundle.macOS.signingIdentity` 设为 `"-"`，打包时对整个 .app 做临时签名。没有这一步时只有链接器给可执行文件加的签名，Info.plist 和资源没有纳入签名，从浏览器下载后系统会直接报“已损坏”，隐私与安全性里也没有放行按钮（0.1.2 的 DMG 就是这样）。
- 本地打包默认不生成更新包，不需要更新签名私钥。
- 发布构建按体积优化（根 `Cargo.toml` 的 `[profile.release]`），图片解码、压缩和哈希相关的 crate 单独保持速度优先。

## 发布流程

每个版本开一个短期的 `release/*` 分支，只放版本号、更新记录、阻塞发布的修复和最后的验证。

1. **写更新记录**：在 [CHANGELOG.md](../CHANGELOG.md) 里写好 `## <版本号>` 小节，写法见文件开头的注释。这个小节会原样显示在应用的更新对话框里，也用作 Release 说明；找不到对应版本时退回“未发布”小节。
2. **改版本号**：`frontend/package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 三处必须一致，不一致时 workflow 直接失败。`crates/core/Cargo.toml` 的版本号习惯上一起改。
3. **提交并推送。**
4. **触发构建**：在 Actions 页面分别运行「构建 macOS 安装包」（[build-macos.yml](../.github/workflows/build-macos.yml)）和「构建 Windows 安装包」（[build-windows.yml](../.github/workflows/build-windows.yml)）。两个都只能手动触发，互相独立。

两个 workflow 都先安装依赖、做类型检查、核对版本号，再打包。Windows 的可以勾选 `validate`，额外运行 Rust 格式检查、Clippy 和测试。`release`（“发布到 Release”）决定发不发版：

- **不勾选**：只构建，产物在运行详情页的 Artifacts 里下载，保留 14 天。
- **勾选**：以 `v<版本号>` 为 tag 创建 Release，tag 指向所选分支的最新提交，说明取 CHANGELOG 里该版本的小节。Release 已存在时只覆盖上传附件，不改说明；旧名字的附件要手动删除。发布后会触发「部署官网」，让官网首屏显示新版本。

| | macOS | Windows |
|---|---|---|
| 安装包 | `sticker-manager_<版本>_aarch64.dmg` | `sticker-manager_<版本>_x64-setup.exe`（NSIS） |
| 更新包 | `sticker-manager_<版本>_aarch64.app.tar.gz` | 安装包本身 |
| 更新清单 | `latest.json` | `latest-windows.json` |
| 其他 | | `sticker-manager_<版本>_windows-x64-portable.zip`（便携版） |

附件都用 ASCII 文件名，因为 GitHub 会去掉附件名里的中文。两个平台的更新清单互不覆盖。

## 检查更新与签名密钥

应用检查的是标为 Latest 的 Release 上的更新清单：macOS 读 `https://github.com/wjiajian/sticker-manager/releases/latest/download/latest.json`，Windows 读同一位置的 `latest-windows.json`。

更新包用 Tauri 的更新签名密钥签名（minisign，与 Apple 签名和 Windows 代码签名都无关），公钥写在 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`，应用只安装签名对得上的更新包。打包时 Tauri 会把版本号写进签名，配置里开了 `requireSignedVersion`，清单声称的版本必须与签名里的一致，旧版本的更新包不能冒充新版本来降级。

- 私钥在作者本机 `~/.tauri/sticker-manager.key`，没有密码，不进仓库。**务必另外备份**：私钥丢了，已安装的应用就再也收不到更新，只能换公钥后让用户手动重装一次。
- 仓库 Settings › Secrets and variables › Actions 里需要 Secret `TAURI_SIGNING_PRIVATE_KEY`，内容是私钥文件的全文；私钥设了密码时再加 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。也可以用命令行：`gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/sticker-manager.key`。
- 有这个 Secret 时，workflow 用 `--config '{"bundle":{"createUpdaterArtifacts":true}}'` 额外生成更新包，再由 `.github/scripts/` 里的脚本改成固定文件名并生成更新清单。没有 Secret 时只能构建，勾选发布会直接失败，避免发出没有更新清单的 Release、让已安装的应用检查更新报错。
- 本地要生成同样的更新包时，在打包命令前加 `TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/sticker-manager.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""`，并加上述 `--config` 和 `--bundles app`。
- 开发构建不自动检查更新。设置 `STICKER_UPDATE_ENDPOINT` 为一个更新清单地址（可以是本机 http）时，改从那里检查并打开自动检查；开发构建只下载和校验签名，不安装。
- Windows 便携版不走应用内更新，检查更新入口只提示手动升级。

## 首次打开

- **macOS**：下载的 DMG 装好后首次打开会被 Gatekeeper 拦下。先打开一次，再到“系统设置 › 隐私与安全性”底部点“仍要打开”。提示“已损坏”或想跳过这一步时，在终端执行 `xattr -cr /Applications/表情管理器.app`。
- **Windows**：会被 SmartScreen 拦一下，点“仍要运行”。

## 试运行发布版

发布版忽略 `STICKER_LIBRARY_DIR`，直接启动会打开真实仓库，首次启动还会登记开机自启。试运行必须隔离。

### macOS：隔离 HOME

```bash
release_home="$(mktemp -d /tmp/sticker-release-home.XXXXXX)"
mkdir -p "$release_home/Library/LaunchAgents" "$release_home/tmp"
env HOME="$release_home" TMPDIR="$release_home/tmp" \
  target/release/bundle/macos/表情管理器.app/Contents/MacOS/sticker-manager
```

- 用二进制的完整路径启动，不要通过 Dock 或访达打开测试副本，否则会丢掉隔离环境。
- 启动后确认偏好设置里的仓库路径在 `release_home` 下。不要在测试副本里填真实的同步账号。退出测试进程后再清理隔离目录。
- 隔离能生效，是因为当前锁定的依赖都优先读 `HOME`：Tauri 用 `dirs 7` 解析数据目录，自启依赖的 `auto-launch 0.6` 用 `dirs 6`，两者共用 `dirs-sys 0.5`。升级这些依赖后要先重新验证，再运行发布版。
- 检查 DMG 用 `hdiutil attach -nobrowse -readonly <DMG 路径>`，看完立即 `hdiutil detach <挂载点>`。
- 在 DMG、App Translocation 和系统临时目录里运行的应用不会登记自启，也不消耗首次登记的机会。要验证搬到稳定位置后的登记，用 `ditto` 把应用复制到 `/Users/Shared/` 下专用目录的 `Applications/` 里，仍用隔离 HOME 启动，测完退出并删除副本。不要把验收副本装进 `/Applications`。

### Windows：用便携版

便携版的仓库在程序旁边的 `data` 目录，也不登记开机自启。把便携 ZIP 解压到一个临时目录运行，就不会碰到安装版的仓库。安装版的安装、卸载和首次运行需要单独安排验收。

## Windows 本地构建

需要 Rust MSVC 工具链、C++ Build Tools、Windows SDK、Node.js 和项目固定版本的 pnpm；运行需要 WebView2 Runtime。

```powershell
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend tauri build --debug --no-bundle --target x86_64-pc-windows-msvc -- --locked
pnpm --dir frontend tauri build --bundles nsis --target x86_64-pc-windows-msvc -- --locked
```

带 `--target` 时，Debug 程序和安装包在 `target/x86_64-pc-windows-msvc/` 下。workflow 用 runner 的默认目标，把安装包和便携 ZIP 整理到 `target/release/bundle/release/`。Debug 程序内嵌前端，可以直接运行。

真实窗口测试用 Debug 程序，并设置绝对路径的临时仓库：

```powershell
$env:STICKER_LIBRARY_DIR = Join-Path $env:TEMP ("sticker-test-" + [guid]::NewGuid())
& .\target\x86_64-pc-windows-msvc\debug\sticker-manager.exe
```

### 便携版

便携 ZIP 里有程序、`portable.flag` 和使用说明。把整个 ZIP 解压到可写目录后运行，首次启动会在旁边创建 `data` 仓库和 `webview` 缓存。

- 便携模式由程序所在目录的 `portable.flag` 启用。
- 不迁移安装版的仓库，不登记开机自启，不走应用内更新。
- 升级时先退出应用，再替换 EXE，保留 `portable.flag`、`data` 和 `webview`。换位置时移动整个目录。

### 图标

Windows 图标复用 `src-tauri/icons/app-icon.svg`，生成时把根 `viewBox` 改为 `72 72 880 880` 以减少任务栏图标的留白，用 `pnpm --dir frontend tauri icon` 生成后，只保留 ICO 和 128 像素 PNG 到 `src-tauri/icons/windows/`。`tauri.windows.conf.json` 使用这一组资源。

Windows 的仓库位置和旧目录迁移见 [ADR 0002](adr/0002-managed-library.md)，同步文件夹的文件名限制见 [tech-stack.md](tech-stack.md#同步)。
