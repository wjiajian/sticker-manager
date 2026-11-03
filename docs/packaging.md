# 打包（macOS 14.0 起）

从源码打包。目前只验证过 macOS，没有签名、公证和自动更新。

```bash
pnpm tauri build --bundles app
CI=true pnpm tauri build --bundles app,dmg
```

产物分别在 `target/release/bundle/macos/表情管理器.app` 和 `target/release/bundle/dmg/`。只要 DMG 时可用 `--bundles dmg`，但 Tauri 会清理作为中间产物的 `.app`；要同时保留两者就用 `--bundles app,dmg`。`CI=true` 跳过 DMG 的访达窗口美化，避免构建时弹出自动化权限提示。当前未配置开发者签名、公证和自动更新。

**试运行发布版必须隔离 HOME**：发布版忽略 `STICKER_LIBRARY_DIR`，直接启动会打开真实仓库，也可能写入真实 `~/Library/LaunchAgents`。目前锁定的 Tauri 使用 `dirs 7` 解析数据目录，自启依赖 `auto-launch 0.6` 使用 `dirs 6` 解析 HOME；两者共用优先读取 `HOME` 的 `dirs-sys 0.5`，已用一次性程序验证。升级这些依赖后应重新验证，再运行发布版。

```bash
release_home="$(mktemp -d /tmp/sticker-release-home.XXXXXX)"
mkdir -p "$release_home/Library/LaunchAgents" "$release_home/tmp"
env HOME="$release_home" TMPDIR="$release_home/tmp" \
  target/release/bundle/macos/表情管理器.app/Contents/MacOS/sticker-manager
```

使用二进制的完整路径启动，不通过 Dock 或访达首次启动测试副本，避免丢失隔离环境。偏好设置中的仓库路径应在 `release_home` 下；不要在测试副本中配置同步或凭据，凭据存在仓库的 local 文件夹里，随隔离 HOME 走，但仍不要用测试副本填真实账号。退出测试进程后才清理隔离目录。

挂载检查可用 `hdiutil attach -nobrowse -readonly <DMG路径>`，检查后立即 `hdiutil detach <挂载点>`。DMG、App Translocation 和系统临时目录中的应用不会自动登记自启，也不会消耗首次登记机会；要验证搬到稳定位置后再登记，可用 `ditto` 复制到 `/Users/Shared/` 下专用目录的 `Applications/` 中，仍用上述隔离 HOME 启动，测试完退出并删除副本。不要把验收副本装进 `/Applications`。
