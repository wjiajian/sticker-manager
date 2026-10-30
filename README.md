# 表情管理器

跨 macOS 与 Windows 的个人表情管理工具：把表情集中收进一个仓库，打标签、快速取用，并在自己的设备之间同步。

- 术语：[GLOSSARY.md](GLOSSARY.md)
- 技术栈与代码结构：[docs/tech-stack.md](docs/tech-stack.md)，关键取舍见 [docs/adr/](docs/adr/)
- 界面设计：[design/README.md](design/README.md)

## 开发

需要 Node 24、pnpm 11、Rust stable；macOS 上还要 Xcode 命令行工具，Windows 上要 WebView2 和 MSVC 生成工具。

```bash
pnpm install
pnpm tauri dev      # 启动应用，前端改动热更新，Rust 改动自动重新编译
```

```bash
cargo test          # 核心库单元测试
pnpm typecheck      # 前端类型检查
pnpm tauri build    # 打包安装程序
```

应用图标的源文件是 `src-tauri/icons/app-icon.svg`，改完后运行 `pnpm tauri icon src-tauri/icons/app-icon.svg` 重新生成各尺寸。
