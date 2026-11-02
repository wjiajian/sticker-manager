# 技术栈

术语见 [GLOSSARY.md](../GLOSSARY.md)；关键取舍的理由见 [docs/adr/](adr/)。

## 一览

| 层 | 选择 |
|---|---|
| 应用外壳 | Tauri 2（[ADR 0001](adr/0001-tauri-2.md)） |
| 界面 | React + TypeScript + Vite |
| 样式 | Tailwind CSS v4，主题变量对应 [设计规范](../design/README.md) |
| 前端状态 | Zustand（界面状态）+ TanStack Query（调用 Rust 命令取数据） |
| 图标 | Phosphor Icons（`@phosphor-icons/react`） |
| 核心逻辑 | Rust，独立库 `crates/core/`，不依赖 Tauri |
| 元数据 | SQLite，只由 Rust 读写 |
| 存储后端接入 | 阻塞文件接口；同步文件夹用 `std::fs`，其余后端下一轮用 OpenDAL |
| 凭据 | 系统钥匙串（macOS 钥匙串 / Windows 凭据管理器），用 `keyring` |
| 悬浮球窗口 | macOS：`tauri-nspanel`；Windows：置顶、不激活窗口 |
| 拖出发送 | `tauri-plugin-drag` |
| 全局快捷键 | `tauri-plugin-global-shortcut` |
| 开机自启 | `tauri-plugin-autostart` |
| 自动更新 | `tauri-plugin-updater`，从 GitHub Releases 拉取 |
| 包管理 | pnpm；Rust workspace |

## 代码结构

```
/
├── src/                 React 界面（主窗口、悬浮球、展开面板）
├── src-tauri/           Tauri 外壳：窗口、托盘、快捷键、剪贴板、拖拽，以及转发给核心库的命令
├── crates/
│   └── core/            仓库、标签、导入、同步与合并规则；可单独测试
├── design/              设计稿与设计说明
└── docs/                技术文档与 ADR
```

同步合并规则是最容易出错的部分，放在 `crates/core/` 里，脱离界面写单元测试。

`src-tauri/permissions/` 里给每个窗口列了允许调用的命令（主窗口 `main.toml`，表情面板 `panel.toml`，悬浮球 `ball.toml`）。有了这个目录，Tauri 会拒绝名单之外的命令，所以新增命令时要同时加进用到它的窗口的名单。

### 代码命名

代码里的英文名与 [GLOSSARY.md](../GLOSSARY.md) 的术语一一对应。GLOSSARY 里的"避免"只约束中文文案和讨论，比如 `sticker` 只出现在标识符里，界面上仍然写"表情"。

| 术语 | 代码里 |
|---|---|
| 表情 | sticker |
| 表情面板 | panel |
| 悬浮球 | ball |
| 偏好设置 | settings |
| 表情包 | pack |
| 仓库 | library |
| 配文 | caption |
| 最近删除 | recently deleted |
| 标签 / 标签分组 | tag / tag group |
| 未打标签 | untagged |
| 收藏 | favorite |
| 使用 / 最近使用 | usage / recent |
| 设备 | device |
| 存储后端 | storage backend |
| 同步文件夹 | sync folder |
| 操作日志 | operation log |

## 仓库与数据

- 仓库位置固定：macOS `~/Library/Application Support/<应用名>/`，Windows `%APPDATA%\<应用名>\`（[ADR 0002](adr/0002-managed-library.md)）。
- 表情文件按内容哈希存放；导入已有的图会合并，提示"新增 N 张，M 张已存在"。
- 第一版支持的格式：PNG、JPG、WebP（含动图）、GIF、APNG；不支持视频。
- 缩略图在每台设备上各自生成，不同步。
- 搜索：对表情名、标签、配文做模糊匹配，不用全文索引；仓库超过 1 万张时再考虑接入中文分词。

## 剪贴板

- **复制**：同时写入位图数据（动图取第一帧）和文件引用（macOS 用文件 URL，Windows 用 CF_HDROP），由目标应用挑能用的那份。
- **复制为文件**（Ctrl+Shift+C / ⌘⇧C）：只写文件引用。如果某个应用粘贴不出动图，就用这个。
- **拖出发送**：在系统层发起拖拽，交出仓库里的原文件。

## 同步

方案见 [ADR 0003](adr/0003-serverless-sync.md)，删除规则见 [ADR 0004](adr/0004-soft-delete-30-days.md)，文件格式和合并细节见 [ADR 0005](adr/0005-sync-format-and-merge.md)。

- **已实现**：同步文件夹，用 `std::fs` 原子发布原图和 JSONL 日志段。`crates/core/src/sync/` 的阻塞 `Storage` 接口只列举、读取、写入、删除文件；内存后端可控制逐文件送达，用于收敛测试。WebDAV、Cloudflare R2、S3 兼容后端及钥匙串凭据下一轮接入。
- **持久化**：SQLite 0002 迁移为所有本机改动记录操作，即使未开启同步也生效；按字段时钟合并，保留永久删除标记和合并映射。加入或切换仓库先推完整快照。

- **会同步**：表情、标签、标签分组、收藏、配文、使用记录（使用次数相加，最近一次使用时间取最晚的）。
- **只在本机**：悬浮球显示、位置和贴边方向、自动收起时间、首次自启初始化标记、同步后端类型、文件夹路径和同步仓库 ID 存在 `<仓库>/local/settings.json`，与 `device.json` 一样不参与同步。开机自启状态每次从系统登记读取，不写入设置文件；全局快捷键和存储后端凭据也只属于本机。
- **触发时机**：
  - 本地改动后 10 秒内没有新改动就推送；
  - 每 5 分钟拉取一次；
  - 启动、从睡眠唤醒、手动点击"立即同步"时各同步一次；
  - 唤醒用每秒墙钟采样检测（相邻样本间隔超过 15 秒）；
  - 单工作线程串行同步，运行中再次触发会接着再跑一次。
- 同步文件夹由同步盘传递文件，本应用不联网；按流量计费暂停留到其他后端接入时实现。
- **加入新设备**：在偏好设置的“同步”分组选择同一同步文件夹；检测到已有仓库后提示设备数、表情数并确认加入。设备名默认用电脑名，可修改，保存在本机 `local/device.json`。标题栏和左栏卡片显示同步状态。
- 第一版不做端到端加密。

## 默认设置

- 全局快捷键：macOS ⌃⌥E，Windows Ctrl+Alt+E，在鼠标所在的屏幕弹出面板。
- 开机自启：发布版首次启动时默认开启，之后尊重用户的系统登记；调试版不自动开启。自启启动参数为 `--autostart`，只显示悬浮球和托盘。
- 悬浮球：开启，初始在主显示器右边、高度 40% 处；静止 3 秒后收起，可改为 10 秒或不收起。

## 发布

- 第一版只给自己用，不签名：macOS 第一次打开时要在"系统设置 › 隐私与安全性"里手动允许；Windows 会被 SmartScreen 拦一下，点"仍要运行"即可。
- 公开发布前再补上：Apple 开发者账号签名和公证、Windows 代码签名证书。

## 以后再做

- 用大模型自动识别配文：只在表情还没有配文时自动填入，不覆盖手动填写的。
- 端到端加密（可选开关）。
- 移动仓库位置。
- 支持视频表情。
