# 打包（macOS 14.0 起）

从源码打包。目前只验证过 macOS，没有 Apple 签名和公证。

```bash
pnpm tauri build --bundles app
CI=true pnpm tauri build --bundles app,dmg
```

产物分别在 `target/release/bundle/macos/表情管理器.app` 和 `target/release/bundle/dmg/`。只要 DMG 时可用 `--bundles dmg`，但 Tauri 会清理作为中间产物的 `.app`；要同时保留两者就用 `--bundles app,dmg`。`CI=true` 跳过 DMG 的访达窗口美化，避免构建时弹出自动化权限提示。当前未配置 Apple 开发者签名和公证；`tauri.conf.json` 的 `bundle.macOS.signingIdentity` 设为 `"-"`，打包时对整个 .app 做临时签名（ad-hoc）。不做这一步时只有编译器给可执行文件加的签名，Info.plist 和资源没有纳入签名，从浏览器下载后系统会直接报“已损坏”，隐私与安全性里也没有放行按钮（0.1.2 的 DMG 就是这样）。本地打包默认不生成更新包，不需要更新签名私钥。

## GitHub Actions 构建

在仓库的 Actions 页面选「构建 macOS 安装包」，点 Run workflow 手动触发。只构建 Apple Silicon（arm64）的 DMG，完成后在运行详情页的 Artifacts 下载，保留 14 天。

运行时勾选「发布到 Release」，会以 `v` 加 `package.json` 的版本号为 tag 创建正式 Release，附上 DMG `sticker-manager_<版本>_aarch64.dmg`、更新包 `sticker-manager_<版本>_aarch64.app.tar.gz` 和 `latest.json`，说明取 `CHANGELOG.md` 里该版本的小节，与应用更新对话框里显示的一致。附件都用 ASCII 文件名，因为 GitHub 会去掉附件名里的中文。该 tag 的 Release 已存在时，只覆盖上传这三个文件，不改说明；旧名字的附件要手动删除。tag 指向运行时选择的分支的最新提交，发布前先确认版本号已更新：`package.json`、`src-tauri/tauri.conf.json` 和 `src-tauri/Cargo.toml` 三处必须一致，不一致时构建直接失败。

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
