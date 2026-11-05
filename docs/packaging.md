# 打包

从源码打包。macOS 和 Windows x64 均已通过构建，Windows 11 运行验收见 [Windows 验收报告](windows-validation.md)。macOS 沿用临时签名，Windows 安装包未签名。

```bash
pnpm --dir frontend tauri build --bundles app
CI=true pnpm --dir frontend tauri build --bundles app,dmg
```

产物分别在 `target/release/bundle/macos/表情管理器.app` 和 `target/release/bundle/dmg/`。只要 DMG 时可用 `--bundles dmg`，但 Tauri 会清理作为中间产物的 `.app`；要同时保留两者就用 `--bundles app,dmg`。`CI=true` 跳过 DMG 的访达窗口美化，避免构建时弹出自动化权限提示。当前未配置 Apple 开发者签名和公证；`tauri.conf.json` 的 `bundle.macOS.signingIdentity` 设为 `"-"`，打包时对整个 .app 做临时签名（ad-hoc）。不做这一步时只有编译器给可执行文件加的签名，Info.plist 和资源没有纳入签名，从浏览器下载后系统会直接报“已损坏”，隐私与安全性里也没有放行按钮（0.1.2 的 DMG 就是这样）。本地打包默认不生成更新包，不需要更新签名私钥。

## GitHub Actions 构建

在仓库的 Actions 页面选「构建 macOS 安装包」，点 Run workflow 手动触发。只构建 Apple Silicon（arm64）的 DMG，完成后在运行详情页的 Artifacts 下载，保留 14 天。

运行时勾选「发布到 Release」，会以 `v` 加 `frontend/package.json` 的版本号为 tag 创建正式 Release，附上 DMG `sticker-manager_<版本>_aarch64.dmg`、更新包 `sticker-manager_<版本>_aarch64.app.tar.gz` 和 `latest.json`，说明取 `CHANGELOG.md` 里该版本的小节，与应用更新对话框里显示的一致。附件都用 ASCII 文件名，因为 GitHub 会去掉附件名里的中文。该 tag 的 Release 已存在时，只覆盖上传这三个文件，不改说明；旧名字的附件要手动删除。tag 指向运行时选择的分支的最新提交，发布前先确认版本号已更新：`frontend/package.json`、`src-tauri/tauri.conf.json` 和 `src-tauri/Cargo.toml` 三处必须一致，不一致时构建直接失败。

## 检查更新与签名密钥

应用检查的是 `https://github.com/wjiajian/sticker-manager/releases/latest/download/latest.json`，也就是标为 Latest 的 Release 上的 `latest.json`。更新包用 Tauri 的更新签名密钥签名（minisign，与 Apple 签名无关），公钥写在 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`，应用只安装签名对得上的更新包。打包时 Tauri 会把版本号写进签名，配置里开了 `requireSignedVersion`，`latest.json` 声称的版本必须与签名里的一致，旧版本的更新包不能冒充新版本来降级。

- 私钥在作者本机 `~/.tauri/sticker-manager.key`，没有密码，不进仓库。**务必另外备份**：私钥丢了，已安装的应用就再也收不到更新，只能换公钥后让用户手动重装一次。
- 仓库 Settings › Secrets and variables › Actions 里需要 Secret `TAURI_SIGNING_PRIVATE_KEY`，内容是私钥文件的全文；私钥设了密码时再加 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。也可以用命令行：`gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/sticker-manager.key`。
- 有这个 Secret 时 workflow 用 `--config '{"bundle":{"createUpdaterArtifacts":true}}'` 额外生成更新包，再由 `.github/scripts/updater-manifest.mjs` 改成固定文件名并生成 `latest.json`，更新说明取 `CHANGELOG.md` 里该版本的小节（没有就取“未发布”）。没有 Secret 时只能构建 DMG，勾选发布会直接失败，避免发出没有 `latest.json` 的 Release、让已安装的应用检查更新报错。
- 本地要生成同样的更新包时，在打包命令前加 `TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/sticker-manager.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""`，并加上述 `--config` 和 `--bundles app`。
- 开发构建不自动检查。设置 `STICKER_UPDATE_ENDPOINT` 为一个 `latest.json` 地址（可以是本机 http）时，开发构建改从那里检查并打开自动检查；开发构建只下载和校验签名，不安装。

CI 构建同样只有临时签名，没有公证，下载的 DMG 装好后首次打开会被 Gatekeeper 拦下：先打开一次，再到「系统设置 › 隐私与安全性」底部点「仍要打开」。提示“已损坏”或想跳过这一步时，在终端执行：

```bash
xattr -cr /Applications/表情管理器.app
```

**试运行发布版必须隔离 HOME**：发布版忽略 `STICKER_LIBRARY_DIR`，直接启动会打开真实仓库，也可能写入真实 `~/Library/LaunchAgents`。目前锁定的 Tauri 使用 `dirs 7` 解析数据目录，自启依赖 `auto-launch 0.6` 使用 `dirs 6` 解析 HOME；两者共用优先读取 `HOME` 的 `dirs-sys 0.5`，已用一次性程序验证。升级这些依赖后应重新验证，再运行发布版。

```bash
release_home="$(mktemp -d /tmp/sticker-release-home.XXXXXX)"
mkdir -p "$release_home/Library/LaunchAgents" "$release_home/tmp"
env HOME="$release_home" TMPDIR="$release_home/tmp" \
  target/release/bundle/macos/表情管理器.app/Contents/MacOS/sticker-manager
```

使用二进制的完整路径启动，不通过 Dock 或访达首次启动测试副本，避免丢失隔离环境。偏好设置中的仓库路径应在 `release_home` 下；不要在测试副本中配置同步或凭据，凭据存在仓库的 local 文件夹里，随隔离 HOME 走，但仍不要用测试副本填真实账号。退出测试进程后才清理隔离目录。

挂载检查可用 `hdiutil attach -nobrowse -readonly <DMG路径>`，检查后立即 `hdiutil detach <挂载点>`。DMG、App Translocation 和系统临时目录中的应用不会自动登记自启，也不会消耗首次登记机会；要验证搬到稳定位置后再登记，可用 `ditto` 复制到 `/Users/Shared/` 下专用目录的 `Applications/` 中，仍用上述隔离 HOME 启动，测试完退出并删除副本。不要把验收副本装进 `/Applications`。

## Windows x64 构建与验收

「构建 Windows 安装包」工作流独立于原有 macOS 构建，只能手动触发。默认检查版本号、安装依赖、类型检查，然后构建 NSIS 和便携 ZIP。勾选 `validate` 可额外运行 Rust 格式、Clippy 和测试。产物统一命名为 `sticker-manager-<版本>-windows-x64`，保留 14 天。没有分支限定的 push 或 PR 自动回归任务。

与 macOS 一样，`release` 默认不勾选；勾选后需要 `TAURI_SIGNING_PRIVATE_KEY`，生成更新签名并创建或补充 `v<版本>` Release。已有 Release 不改说明。Windows 使用独立的 `latest-windows.json`，macOS 工作流和 `latest.json` 保持原样。便携 ZIP 不走安装式更新。更新签名不是 Windows 代码签名。

Windows 构建命令如下，需要 Rust MSVC 工具链、C++ Build Tools、Windows SDK、Node.js 和项目固定版本的 pnpm。

```powershell
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend tauri build --debug --no-bundle --target x86_64-pc-windows-msvc -- --locked
pnpm --dir frontend tauri build --bundles nsis --target x86_64-pc-windows-msvc -- --locked
```

上述带 `--target` 的本地命令把 Debug 程序和安装包放在 `target/x86_64-pc-windows-msvc/` 下。工作流使用 runner 默认 Windows 目标，将安装包和便携 ZIP 整理到 `target/release/bundle/release/`，使用 ASCII 文件名上传。Debug 程序内嵌前端，可按上述命令单独构建。

本机要求所有新增环境安装文件位于 D 盘时，开发工具及可配置缓存使用 `D:\Environment`。微软安装器的一部分组件会安装到系统盘，因此本机使用前端检查及 CI 下载产物进行验收，MSVC 编译使用远端 runner。便携 GitHub CLI 的配置目录通过 `GH_CONFIG_DIR` 指向 D 盘；环境激活脚本只为当前进程设置路径和缓存。

真实窗口验收使用 Debug 程序，并设置独立的绝对路径仓库。

```powershell
$env:STICKER_LIBRARY_DIR = Join-Path 'D:\Environment\sticker-manager-tests' ([guid]::NewGuid().ToString())
& .\target\x86_64-pc-windows-msvc\debug\sticker-manager.exe
```

发布版忽略 `STICKER_LIBRARY_DIR`，首次启动还会初始化开机自启，发布版的安装运行验收应单独安排。拖拽功能采用代码检查；系统设置相关场景由用户切换后验证。

Windows 同步文件夹会在记录移动意图前检查非法字符、结尾空格或句点、设备保留名和过长名称，并给出错误提示。用户可修改对应的表情名或分类名后重试，旧客户端留下的非法目标移动意图也可在改名后重新规划。网络存储沿用原有命名规则。

### Windows 便携版

同一工作流的 `sticker-manager-<版本>-windows-x64` 产物包含 NSIS 安装包和 `sticker-manager_<版本>_windows-x64-portable.zip`；便携 ZIP 内有程序、`portable.flag` 和使用说明。将整个 ZIP 解压到可写目录后运行，首次启动创建旁边的 `data` 仓库和 `webview` 缓存。运行需要 WebView2 Runtime。

便携模式由程序所在目录的 `portable.flag` 文件启用，安装版仓库迁移仅在安装模式执行。便携版关闭开机自启登记，检查更新入口提示手动升级。升级时先退出应用，再替换 EXE，保留 `portable.flag`、`data` 和 `webview`。移动到另一位置时一起移动整个目录。

Windows 安装版仓库为 `%APPDATA%\sticker-manager`。目标为空时优先迁移 `com.wjiajian.sticker-manager`，其次兼容旧的 `com.shaojian.sticker-manager`。迁移失败会报告启动错误。

Windows 图标复用 `icons/app-icon.svg`，生成时将根 `viewBox` 改为 `72 72 880 880`，用 `pnpm --dir frontend tauri icon` 生成后保留 ICO 与 128 像素 PNG 到 `icons/windows/`。Windows 平台配置使用这一组资源，以减少任务栏图标内部留白。
