<div align="center">

<img src="src-tauri/icons/icon.png" alt="表情管理器" width="128" height="128">

# 表情管理器

把表情集中收进一个仓库，归类、快速取用，并在自己的设备之间同步。

![版本 0.1.1](https://img.shields.io/badge/版本-0.1.1-blue)
![平台 macOS 14+](https://img.shields.io/badge/平台-macOS%2014%2B-lightgrey)
[![许可证 MIT](https://img.shields.io/badge/许可证-MIT-green)](LICENSE)

</div>

表情包散落在聊天记录、下载文件夹和各种相册里，想用的时候翻半天。表情管理器是一个个人桌面应用：所有表情放进一个本地仓库，按分类和配文找，一个快捷键呼出面板，单击就能复制或拖出去发送。

> [!NOTE]
> 目前只在 macOS 14 及以上验证过，Windows 还没有编译和运行过。还没有发布安装包，需要[自己打包](docs/packaging.md)；应用没有签名，第一次打开要在“系统设置 › 隐私与安全性”里手动允许。

## 功能

- **收进仓库**：从文件、文件夹、zip 导入，也可以直接拖入；同一张图自动去重。
- **整理**：分类、配文、收藏；按静态或动图筛选，按最近添加、最近使用、使用次数或名称排序。
- **找到并取用**：按名称、配文和分类搜索；一键复制、拖出发送、在访达中显示；常用的排在最近使用条里。
- **随手取用**：全局快捷键 `⌃⌥E` 呼出表情面板，屏幕边上的悬浮球和菜单栏托盘，悬停预览大图，开机自启。
- **同步**：不需要服务器。用同步文件夹（网盘目录）、WebDAV、Cloudflare R2 或 S3 兼容存储在多台设备间同步，可暂停，也可在按流量计费的网络上自动暂停。
- **删除不怕手滑**：先进入最近删除，30 天内可恢复。
- **其他**：跟随系统的浅色和深色外观；同步的账号密码只存本机仓库的 `local/` 文件夹，不同步、不进设置文件。

## 快速开始

1. 打开应用，点右上角“导入表情”，选文件、文件夹或 zip；也可以直接把图片拖进窗口。
2. 在详情栏给表情设分类、写配文。
3. 在任何应用里按 `⌃⌥E` 呼出面板，搜索后单击表情复制，再粘贴到聊天里。
4. 有多台设备的话，在“偏好设置 › 同步”里选一种存储，另一台设备连到同一个地方就会合并。

仓库默认放在 `~/Library/Application Support/com.shaojian.sticker-manager/`，同步只传表情和它的整理信息，不上传到任何第三方服务，除非你自己选了某个云存储。

## 开发

需要 Node 24、pnpm 11、Rust stable；macOS 上还要 Xcode 命令行工具。

```bash
pnpm install
pnpm tauri dev      # 启动应用，前端热更新，Rust 改动自动重新编译
```

开发时默认会打开真实仓库。想用一个临时仓库调试，把 `STICKER_LIBRARY_DIR` 设成绝对路径，目录不存在会自动新建；这个变量只在开发构建里生效。

```bash
STICKER_LIBRARY_DIR="$(mktemp -d)" pnpm tauri dev
```

```bash
cargo test --workspace   # Rust 测试
pnpm typecheck           # 前端类型检查
```

### 目录

| 路径 | 内容 |
|---|---|
| `src/` | 前端（React + TypeScript），主窗口、面板、悬浮球和预览各一个入口 |
| `crates/core/` | 核心逻辑（Rust）：仓库、导入、搜索、删除、同步，不依赖 Tauri |
| `src-tauri/` | 应用外壳：窗口、托盘、快捷键、剪贴板、同步凭据 |
| `design/` | 界面设计规范 |
| `docs/` | 技术栈、架构决策、测试、打包、术语表和待办 |

## 文档

- [技术栈与代码结构](docs/tech-stack.md)
- [界面设计](design/README.md)
- [打包](docs/packaging.md)　
- [测试](docs/testing.md)
- [术语表](docs/GLOSSARY.md)　
- [更新记录](CHANGELOG.md)　
- [待办](docs/TODO.md)
