# 待办

还没做、但已经决定要做的事。做完一项就删掉，并把结论写进 [design/README.md](design/README.md) 或 [docs/tech-stack.md](docs/tech-stack.md)。还没决定做的设想见 tech-stack 的“远期设想”。

## 以后再做

- **检查更新**：`tauri-plugin-updater` 从 GitHub Releases 拉取（tech-stack 已选定，代码里还没有）。要先定更新入口（偏好设置、托盘）、是否自动检查和提示方式，并准备 Releases 的签名密钥。
- **剪贴板格式**：2026-11 的探索只测了 Chrome、Safari、文本编辑和备忘录，倾向于“复制静态图只写 PNG，复制为文件只写文件 URL”（[记录](docs/reports/2026-11-02-appearance-purge-compaction.md)）。改之前要在微信、QQ、飞书里验证：QQ 粘出文件卡片的问题能不能解决，飞书的 GIF 还能不能动。动图怎么写、要不要按前台应用分流，等实际碰到问题再定。
- **日志检查点（丢弃历史）**：目前只合并日志段来减少请求数，历史字节全部保留，新设备首次加入仍要读完全部历史。做检查点要先定安全的历史淘汰、旧设备回归和版本兼容规则。
- **Windows**：从没编译和运行过。要做的包括自定义标题栏（设计稿 MainWin）、剪贴板、按流量计费检测、悬浮球与预览窗口的实际表现、深色外观。
- **菜单栏中文化**：macOS 菜单栏还是 Tauri 默认的英文菜单（File / Edit / View / Window / Help）。
- **多选和撤销**：表情网格多选、删除后撤销。
- **修改快捷键、手动选择悬浮球贴边位置**：设计稿标为待设计。
- **发布前**：设置最低 macOS 版本（预览窗口用到的 `NSApplication::activate` 需要 macOS 14）、Apple 签名公证、Windows 代码签名。
