<div align="center">

<img src="src-tauri/icons/icon.png" alt="表情管理器" width="128" height="128">

# 表情管理器

macOS 和 Windows 上的表情包管理器，把表情收进一个仓库，分类整理，用的时候随手就能取。

[![版本 0.1.7](https://img.shields.io/badge/版本-0.1.7-blue)](https://github.com/wjiajian/sticker-manager/releases/latest)
![平台 macOS 14+](https://img.shields.io/badge/平台-macOS%2014%2B-lightgrey)
![平台 Windows x64](https://img.shields.io/badge/平台-Windows%20x64-lightgrey)
[![许可证 MIT](https://img.shields.io/badge/许可证-MIT-green)](LICENSE)
<a href="https://linux.do" alt="LINUX DO"><img src="https://shorturl.at/ggSqS" /></a>

[官网](https://wjiajian.github.io/sticker-manager/) · [下载](https://github.com/wjiajian/sticker-manager/releases/latest) · [使用文档](https://wjiajian.github.io/sticker-manager/docs/) · [开发者篇](#开发者篇) · [更新记录](CHANGELOG.md)

</div>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/main-window-dark.png">
  <img src="docs/images/main-window.png" alt="表情管理器主窗口：左栏是仓库和分类，中间是表情网格，右侧详情栏显示选中表情的分类、配文和使用记录">
</picture>

表情包散落在聊天记录、下载文件夹和各种相册里，想用的时候要翻半天。表情管理器是一个个人桌面应用：所有表情放进一个本地仓库，按分类、名称和配文来找；在任何应用里按一下快捷键呼出表情面板，单击复制或者直接拖出去发送。有多台电脑的话，可以通过自己的网盘或云存储同步，不需要服务器。

<table>
<tr>
<td width="300"><img src="docs/images/panel-search.gif" alt="表情面板搜索演示：依次输入「开心」和「困惑」，结果随输入更新，方向键移动选中项" width="300"></td>
<td>

- **随手取用**：在任何应用里按 `⌃⌥E`（Windows 是 `Ctrl+Alt+E`）或者单击悬浮球呼出表情面板，直接输入就能搜索，单击复制，也可以拖进聊天窗口。
- **发出去的是原图**：支持 PNG、JPG、WebP、GIF、APNG，复制和拖出时保留原来的格式、透明背景和动画。
- **整理省事**：每张表情归到一个分类，还可以写配文、点收藏。导入文件夹时按文件夹名自动分类，同一张图只存一份。
- **删错了能找回**：删除的表情在「最近删除」里保留 30 天。
- **同步用自己的存储**：支持同步文件夹（iCloud 云盘、Dropbox 等）、WebDAV、Cloudflare R2 和 S3 兼容存储。
- **浅色和深色**：默认跟随系统，也可以固定一种，主窗口、表情面板和悬浮球一起切换。

</td>
</tr>
</table>

<sub>截图和录屏来自 macOS 版。</sub>

- [使用文档](#使用文档)：[安装](https://wjiajian.github.io/sticker-manager/docs/) · [快速上手](https://wjiajian.github.io/sticker-manager/docs/quick-start/) · [常见问题](https://wjiajian.github.io/sticker-manager/docs/faq/)
- [开发者篇](#开发者篇)：[技术栈](#技术栈) · [目录结构](#目录结构) · [本地开发](#本地开发) · [提交前检查](#提交前检查) · [发布流程](#发布流程) · [贡献](#贡献)

## 使用文档

安装和使用说明都在[官网](https://wjiajian.github.io/sticker-manager/docs/)上：

- [安装](https://wjiajian.github.io/sticker-manager/docs/)：下载哪个文件，第一次打开时怎样放行
- [快速上手](https://wjiajian.github.io/sticker-manager/docs/quick-start/)：导入表情，呼出表情面板
- 功能：[导入](https://wjiajian.github.io/sticker-manager/docs/import/) · [分类和整理](https://wjiajian.github.io/sticker-manager/docs/organize/) · [表情面板](https://wjiajian.github.io/sticker-manager/docs/panel/) · [悬浮球和托盘](https://wjiajian.github.io/sticker-manager/docs/floating-ball/) · [偏好设置](https://wjiajian.github.io/sticker-manager/docs/preferences/) · [同步](https://wjiajian.github.io/sticker-manager/docs/sync/) · [检查更新](https://wjiajian.github.io/sticker-manager/docs/updates/)
- [数据和备份](https://wjiajian.github.io/sticker-manager/docs/data/)：表情存在哪里，怎样备份和恢复
- [常见问题](https://wjiajian.github.io/sticker-manager/docs/faq/)：打不开、同步提示等问题

## 开发者篇

### 技术栈

| 部分 | 选择 |
|---|---|
| 应用外壳 | [Tauri 2](https://tauri.app/) |
| 核心库 | Rust，`crates/core`（crate 名 `sticker-core`）：仓库、分类、导入、搜索、删除、同步和合并规则。不依赖 Tauri，可以单独测试 |
| 桌面壳 | `src-tauri`：Tauri 命令、窗口、托盘、悬浮球、表情面板、悬停预览、剪贴板、开机自启和应用内更新 |
| 前端 | React + TypeScript + Vite，Tailwind CSS v4，Zustand（界面状态）+ TanStack Query（调用 Rust 命令取数据），Phosphor Icons |
| 元数据 | SQLite（rusqlite），只由 Rust 读写 |
| 同步网络层 | OpenDAL（WebDAV、S3 / R2）+ reqwest（rustls），不依赖 OpenSSL |
| 包管理 | pnpm；Rust workspace（`crates/core`、`src-tauri`），edition 2024 |

前端是多入口的 Vite 应用，每个窗口一个 HTML 入口：

| 入口 | 窗口 | 代码 |
|---|---|---|
| `frontend/index.html` | 主窗口 | `frontend/src/windows/main/` |
| `frontend/panel.html` | 表情面板 | `frontend/src/windows/panel/` |
| `frontend/ball.html` | 悬浮球 | `frontend/src/windows/ball/` |
| `frontend/preview.html` | 悬停预览 | `frontend/src/windows/preview/` |

每个窗口能调用哪些 Tauri 命令，由 `src-tauri/permissions/*.toml` 里的白名单决定（`main.toml`、`panel.toml`、`ball.toml`、`preview.toml`），再由 `src-tauri/capabilities/*.json` 授予对应窗口。名单之外的命令会被 Tauri 拒绝，所以**新增命令时要同时加进用到它的窗口的名单**。更新插件只在 Rust 端调用，不对前端开放。

更详细的说明（剪贴板策略、同步触发时机、外观、默认设置等）见 [docs/tech-stack.md](docs/tech-stack.md)。

### 目录结构

```text
.
├── frontend/                    前端：依赖和 Vite、TypeScript 配置都在这里
│   ├── index.html, panel.html, ball.html, preview.html   各窗口入口
│   └── src/
│       ├── windows/{main,panel,ball,preview}/   各窗口的 React 代码
│       ├── components/                          通用组件
│       ├── lib/                                 IPC 封装、查询、平台判断等
│       ├── stores/                              Zustand 状态
│       └── styles/global.css                    主题变量（浅色和深色）
├── crates/core/                 sticker-core：核心逻辑
│   └── src/
│       ├── import.rs, import/   导入（文件、文件夹、zip）
│       ├── library.rs           打开仓库、列出表情
│       ├── organization.rs      分类、改名、配文
│       ├── usage.rs             使用记录、收藏
│       ├── search.rs            搜索
│       ├── deletion.rs          最近删除和彻底删除
│       ├── sync/                同步：存储接口、目录树、操作日志、合并
│       └── migrations/          SQLite 迁移
├── src-tauri/                   Tauri 桌面壳
│   ├── src/                     命令、窗口、托盘、悬浮球、面板、更新等
│   ├── permissions/             各窗口的命令白名单
│   ├── capabilities/            把权限授予各窗口
│   ├── icons/                   应用和托盘图标
│   └── tauri.conf.json
├── design/README.md             界面设计规范
├── docs/                        技术文档、ADR、术语表、测试、打包、待办、README 配图
├── website/                     软件官网和使用文档（Astro + Starlight）
└── .github/                     macOS／Windows 独立构建与发布 workflow、更新清单脚本
```

### 本地开发

#### 环境要求

- macOS 14 及以上，装好 Xcode 命令行工具；或 Windows x64，装好 C++ Build Tools、Windows SDK 和 WebView2 Runtime（见[打包说明](docs/packaging.md#windows-x64-构建与验收)）
- Rust stable（workspace 使用 edition 2024）
- Node.js（CI 用 22）
- pnpm 11（`frontend/package.json` 的 `packageManager` 固定为 `pnpm@11.22.0`）

#### 安装依赖并启动

前端依赖和脚本都在 `frontend/` 里，下面的命令在仓库根目录运行：

```bash
pnpm --dir frontend install
```

```bash
pnpm --dir frontend tauri dev
```

`tauri dev` 会先执行 `tauri.conf.json` 里的 `beforeDevCommand`（`pnpm dev`，在 1420 端口启动 Vite），再编译并启动应用。前端改动热更新，Rust 改动自动重新编译。

`frontend/package.json` 里的脚本（在 `frontend/` 里运行，或者在根目录加上 `--dir frontend`）：

| 脚本 | 作用 |
|---|---|
| `pnpm dev` | 只启动 Vite 开发服务器，一般由 `tauri dev` 调用 |
| `pnpm build` | `tsc && vite build`，构建前端到 `frontend/dist/` |
| `pnpm typecheck` | `tsc` 类型检查 |
| `pnpm tauri <命令>` | 回到仓库根目录调用 Tauri CLI，比如 `dev`、`build`；相对路径参数按根目录算 |

本地打包 DMG 的命令和注意事项见 [docs/packaging.md](docs/packaging.md)。

#### 用 `STICKER_LIBRARY_DIR` 隔离仓库

> [!WARNING]
> 开发构建默认打开的是真实仓库（见[数据和备份](https://wjiajian.github.io/sticker-manager/docs/data/)）。调试和测试时请换成临时仓库。

```bash
STICKER_LIBRARY_DIR="$(mktemp -d)" pnpm --dir frontend tauri dev
```

- 必须是绝对路径。相对路径会直接报启动错误，因为它会落在 `src-tauri/` 下，写入的文件又会触发 `tauri dev` 重新编译。
- 目录不存在时会自动新建。
- 只在开发构建里生效；release 构建会忽略这个变量。macOS 打包版通过隔离 `HOME` 验收；Windows 便携版通过程序旁的 `portable.flag` 和临时程序目录隔离，做法见 [docs/packaging.md](docs/packaging.md)。

另外两个只在开发构建里生效的调试变量：

| 变量 | 作用 |
|---|---|
| `STICKER_UPDATE_ENDPOINT` | 改从指定的 `latest.json` 地址检查更新，并打开自动检查；开发构建只下载和校验签名，不安装 |
| `STICKER_FORCE_METERED=1` | 把当前网络当作按流量计费，用来测试自动暂停同步 |

### 提交前检查

```bash
cargo fmt --all --check
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo test --workspace
```

```bash
pnpm --dir frontend typecheck
```

```bash
pnpm --dir frontend build
```

```bash
pnpm --dir website build
```

```bash
git diff --check
```

macOS 与 Windows 分别使用独立、手动触发的安装包构建工作流，均运行类型检查和打包；Windows 可勾选 `validate` 额外运行 Rust 格式、Clippy 和测试。WebDAV 和 S3 的网络集成测试默认忽略，需要本机容器，跑法见 [docs/testing.md](docs/testing.md)。

### 发布流程

macOS 继续使用「构建 macOS 安装包」（[build-macos.yml](.github/workflows/build-macos.yml)）；Windows 使用独立的「构建 Windows 安装包」（[build-windows.yml](.github/workflows/build-windows.yml)）。两者均手动触发，默认只生成构建产物，勾选 `release` 才发布。下列步骤以 macOS 为例。

1. **写更新记录**：在 [CHANGELOG.md](CHANGELOG.md) 里写好这个版本的 `## <版本号>` 小节。写法见文件开头的注释：面向用户，只写能感知到的变化，每条一句话，升级须知放在最前面，不用 `###` 标题。这个小节会原样显示在应用的更新对话框里，也会用作 Release 说明；找不到对应版本时退回「未发布」小节。
2. **改版本号**：`frontend/package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 三处必须一致，不一致时 workflow 直接失败。`crates/core/Cargo.toml` 的版本号习惯上一起改。
3. **提交并推送**。
4. **触发构建**：在仓库的 Actions 页面选「构建 macOS 安装包」，点 Run workflow。`release` 勾选项（「发布到 Release」）决定发不发版：
   - 不勾选：只构建，DMG 在运行详情页的 Artifacts 里下载，保留 14 天。
   - 勾选：以 `v<版本号>` 为 tag 创建 Release，tag 指向所选分支的最新提交。附件是 `sticker-manager_<版本>_aarch64.dmg`、更新包 `sticker-manager_<版本>_aarch64.app.tar.gz` 和 `latest.json`。如果这个 tag 的 Release 已经存在，只覆盖上传这三个文件，不改说明。

Windows 采用相同的版本检查、可选更新签名和 Release 上传流程，附件为 NSIS 安装包、便携 ZIP 和独立的 `latest-windows.json`，不覆盖 macOS 的 `latest.json`。便携版始终手动升级。

附件都用 ASCII 文件名，因为 GitHub 会去掉附件名里的中文。

发布 Release 后，两个工作流都会触发「部署官网」（[deploy-website.yml](.github/workflows/deploy-website.yml)），让官网首屏显示新版本。

### 贡献

- 界面文案和讨论使用 [GLOSSARY.md](docs/GLOSSARY.md) 里的术语
- 测试实例一律用 `STICKER_LIBRARY_DIR` 指向临时目录（绝对路径），不碰真实仓库；试运行发布版时隔离 `HOME`。
- 测试时不修改系统设置（低数据模式、减少动态效果、外观等）。需要的话请用户自己切换。
- 新增 Tauri 命令时，记得加进用到它的窗口在 `src-tauri/permissions/` 里的白名单。
- 其余约定见 [AGENTS.md](AGENTS.md)。
