# 待办

还没做、但已经决定要做的事。做完一项就删掉，并把结论写进 [design/README.md](design/README.md) 或 [docs/tech-stack.md](docs/tech-stack.md)。更远的设想见 tech-stack 的“以后再做”。

## 以后再做

- **检查点（丢弃历史）未做**：目前只合并日志段以减少请求数，保留全部历史字节；新设备首次加入仍要读完历史。检查点需要定义安全的历史淘汰、旧设备回归和版本兼容规则。

- **检查更新**：`tauri-plugin-updater` 从 GitHub Releases 拉取（tech-stack 已选定，代码里还没有）。要先定更新入口（偏好设置、托盘）、是否自动检查和提示方式，并准备 Releases 的签名密钥。
- **Windows**：从没编译和运行过。要做的包括自定义标题栏（设计稿 MainWin）、剪贴板、按流量计费检测、悬浮球与预览窗口的实际表现。
- **菜单栏中文化**：macOS 菜单栏还是 Tauri 默认的英文菜单（File / Edit / View / Window / Help）。
- **多选和撤销**：表情网格多选、删除后撤销。
- **修改快捷键、手动选择悬浮球贴边位置**：设计稿标为待设计。
- **发布前**：设置最低 macOS 版本（预览窗口用到的 `NSApplication::activate` 需要 macOS 14）、Apple 签名公证、Windows 代码签名。
