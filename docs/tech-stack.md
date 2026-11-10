# 技术栈

术语见 [GLOSSARY.md](GLOSSARY.md)；关键取舍的理由见 [docs/adr/](adr/)。

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
| 存储后端接入 | 阻塞文件接口；同步文件夹用 `std::fs`，WebDAV / S3 / R2 用 OpenDAL 0.59.5 |
| 凭据 | 本机仓库 `local/credentials.json`（权限只限当前用户，不同步） |
| 悬浮球窗口 | macOS：`tauri-nspanel`；Windows：置顶、不激活窗口 |
| 拖出发送 | `tauri-plugin-drag` |
| 全局快捷键 | `tauri-plugin-global-shortcut` |
| 开机自启 | `tauri-plugin-autostart` |
| 自动更新 | `tauri-plugin-updater`，读 GitHub Releases 最新版本附带的 `latest.json`；只在 Rust 端调用，前端不开放插件命令（发布流程见 [packaging.md](packaging.md)） |
| 包管理 | pnpm；Rust workspace |

## 代码结构

```
/
├── frontend/            React 界面（主窗口、悬浮球、展开面板、悬停预览）及前端配置
├── src-tauri/           Tauri 外壳：窗口、托盘、快捷键、剪贴板、拖拽，以及转发给核心库的命令
├── crates/
│   └── core/            仓库、分类、导入、同步与合并规则；可单独测试
├── design/              界面设计规范
├── docs/                技术文档、ADR、术语表、待办
└── website/             软件官网（占位，尚未搭建）
```

同步合并规则是最容易出错的部分，放在 `crates/core/` 里，脱离界面写单元测试。

`src-tauri/permissions/` 里给每个窗口列了允许调用的命令（主窗口 `main.toml`，表情面板 `panel.toml`，悬浮球 `ball.toml`，悬停预览 `preview.toml`）。有了这个目录，Tauri 会拒绝名单之外的命令，所以新增命令时要同时加进用到它的窗口的名单。

表情面板的预览使用 `frontend/preview.html` 独立入口，启动时创建隐藏窗口。macOS 使用不能成为 key/main 的非激活 NSPanel，层级比表情面板高一级、跨桌面与全屏并忽略鼠标；Windows 使用不可聚焦、置顶、不进任务栏、忽略鼠标的透明窗口。窗口固定为 296×392，卡片宽 168，CSS 绘制阴影并保留透明边距；页面只接收名称、次数、分类名和图片路径，不查询仓库。

`capabilities/preview.json` 只授权事件监听、取消监听和 `preview_session` / `preview_ready`；面板额外允许 `show_preview` / `hide_preview`。原生层记录面板本次打开来源，在 `geometry.rs` 中计算卡片位置，macOS 全程使用 AppKit 左下原点的逻辑坐标；页面以 CSS 像素报告格子位置。单调递增的请求编号阻止旧解码回执重新显示或移动窗口。

### 代码命名

代码里的英文名与 [GLOSSARY.md](GLOSSARY.md) 的术语一一对应。GLOSSARY 里的"避免"只约束中文文案和讨论，比如 `sticker` 只出现在标识符里，界面上仍然写"表情"。

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
| 分类 | category |
| 未分类 | uncategorized |
| 收藏 | favorite |
| 使用 / 最近使用 | usage / recent |
| 设备 | device |
| 存储后端 | storage backend |
| 同步文件夹 | sync folder |
| 操作日志 | operation log |

## 外观

- 原生窗口主题是唯一来源（`src-tauri/src/appearance.rs`）：切换时同时调 `AppHandle::set_theme` 和每个窗口的 `set_theme`，跟随系统传 `None`。CSS 只写 `@media (prefers-color-scheme: dark)`，前端不另存主题状态。
- macOS 上实测 WKWebView 的 `prefers-color-scheme` 跟着窗口主题变，隐藏的窗口也会变。Windows 11 已验证应用内切换深浅外观可更新主窗口、面板、预览和悬浮球的 WebView2 配色。
- 主窗口先隐藏创建，原生背景色按当前主题设好，第一次页面载入完成后才显示；之后主题变化时同步更新背景色。面板、预览和悬浮球是透明窗口，不设背景色。
- 颜色变化不做 CSS 过渡：隐藏的 WebView 会暂停动画，切换外观后再打开，会先看到从旧颜色淡入。
- 配色、对比度和变量用途见[设计规范](../design/README.md)。

## 仓库与数据

- macOS 仓库位于 `~/Library/Application Support/com.wjiajian.sticker-manager/`，Windows 安装版位于 `%APPDATA%\sticker-manager\`。Windows 首次启动可从旧标识符目录自动迁移，目标已有内容时保留目标仓库。便携版使用程序旁的 `data` 目录，启用方式见[打包说明](packaging.md)。
- 表情文件按内容哈希存放；导入已有的图会合并，提示"新增 N 张，M 张已存在"。
- 第一版支持的格式：PNG、JPG、WebP（含动图）、GIF、APNG；不支持视频。
- 支持直接选中或拖入 `.zip`（zip 9.2.2，关闭默认 features、仅 `deflate`，纯 Rust）。包内递归识别图片，隐藏 / 系统文件和 `__MACOSX/` 忽略；嵌套压缩包及其他格式跳过，文件夹中的 `.zip` 不展开。文件名优先 UTF-8，非 UTF-8 用 GB18030 解码（encoding_rs）。不支持加密及 Deflate / Stored 以外的压缩方式；实际解压量限制为单条目 100 MB、整包 2 GB、10000 条目。
- 压缩包逐条解压到 `objects/` 里的临时文件后立即导入，同一时间只有一个临时文件（macOS 图形应用默认只能同时打开 256 个文件）。条目读完时校验 CRC，损坏的条目只算它自己失败，其余照常导入；目录区读不出来时整包失败。取消在条目之间生效，已导入的保留。
- 导入在锁外读取、哈希、解码和生成缩略图，逐张在短事务内重新查重并提交；准备阶段先短暂持锁查重，已有同图且原图和缩略图都在时跳过解码；原图发布仍按 Library → files 加锁，与同步清理协调。全窗口单批互斥，约 150ms 推送进度、每秒通知仓库变化，支持取消和给整批结果设分类；导入文件夹时新表情按所在那一层文件夹名归入分类，已在库里的不改。
- 缩略图在每台设备上各自生成，不同步。
- 启动时和每小时清理一次没有表情记录的原图、缩略图，以及中断留下的 `.tmp` 临时文件（例如彻底删除时删文件失败留下的）。文件超过 24 小时没改动才清理，给同步“先下载原图、再合并日志写记录”留出时间；只认按哈希命名的文件，其他文件不动。
- 搜索：对表情名、分类、配文做模糊匹配，不用全文索引；仓库超过 1 万张时再考虑接入中文分词。

## 剪贴板

- **复制**：macOS 统一只写一个含 `public.file-url` 的剪贴板条目，指向按表情名命名的原文件副本。不解码、不取动图首帧、不附 PNG，保留源文件的格式、透明通道和动画数据；目标应用可能呈现为图片或文件卡片，发送后是否播放仍由目标应用决定。主窗口按钮、⌘C 和悬浮面板单击使用同一策略。
- **复制为文件**：macOS 已合并进“复制”，不再单列菜单项；⌘⇧C 保留为同一操作的快捷键。Windows 的 Ctrl+Shift+C 仍只写文件引用。
- Windows 已验证普通复制写入 CF_DIB、PNG、CF_HDROP，复制为文件只写 CF_HDROP，透明 PNG 的 alpha 和原 GIF 字节均保留。聊天应用内的粘贴与发送效果待人工验收，格式策略沿用现有实现。详见 [Windows 验收报告](windows-validation.md)。
- **拖出发送**：在系统层发起拖拽，交出副本。
- **在访达 / 资源管理器中显示**：显示副本。
- 以上交出的文件引用都指向临时目录里按表情名命名的副本，保留 7 天，不把仓库里按哈希命名的原文件交给其他应用或用户改动。
- 依据：2026-11-03 在 QQ 和飞书自己的会话里，文件 URL + PNG、只有 PNG、只有文件 URL 都能粘成图片缩略图，去掉 PNG 没有损失；飞书只有带文件 URL 时才引用原 GIF。代价是 Safari、文本编辑粘不出来（旧应用实测），接受这项取舍，不按前台应用分流。微信的外观、飞书的透明度和发送后的动画还要人工确认（见 [TODO.md](TODO.md)）。

## 同步

方案见 [ADR 0003](adr/0003-serverless-sync.md)，删除规则见 [ADR 0004](adr/0004-soft-delete-30-days.md)，文件格式和合并细节见 [ADR 0005](adr/0005-sync-format-and-merge.md)。网络后端的容器集成测试怎么跑见 [testing.md](testing.md)。

- **已实现**：同步文件夹、通用 WebDAV、Cloudflare R2 和 S3 兼容后端，共用 `crates/core/src/sync/` 的阻塞 `Storage` 接口及原有同步调度。网络层采用 OpenDAL 0.59.5、reqwest + rustls，不依赖 OpenSSL；连接与请求有按传输量放宽的超时、指数退避、Retry-After（超过 60 秒结束本轮）及取消。WebDAV 直接 PUT，父目录缺失时才建目录，读取方校验半截文件；内存后端用于逐文件延迟和收敛测试。
- **格式 2**：原图写到分类目录，未分类写根目录；元数据位于 `.sticker-manager/`，最近删除原图位于其 `trash/`。先拉取目录再推送，远端路径变化优先；旧同步仓库需要重新创建。目录缓存按路径、大小、ETag 或修改时间判断差异，没有版本信息时接受同大小替换可能漏检。
- **持久化**：0005 追加本机路径缓存与可重试移动意图，表情 category_id 不再进入日志。SQLite 0002 迁移为所有本机改动记录操作，即使未开启同步也生效；按字段时钟合并，保留永久删除标记和合并映射。加入或切换仓库先推完整快照。
- **会同步**：表情、分类、收藏、配文、使用记录（使用次数相加，最近一次使用时间取最晚的）。
- **只在本机**：外观（跟随系统 / 浅色 / 深色）、悬浮球显示、位置和贴边方向、自动收起时间、首次自启初始化标记、同步后端类型、地址 / bucket / 路径 / region 等非机密参数和同步仓库 ID 存在 `<仓库>/local/settings.json`，与 `device.json` 一样不参与同步。开机自启状态每次从系统登记读取，不写入设置文件；全局快捷键也只属于本机。存储凭据（包括用户名和 Access Key ID）以明文写入 `<仓库>/local/credentials.json`（Unix 下权限 0600，临时文件写好后改名替换），不进入设置文件或 IPC 返回值；按设备 id 和目标隔离，读取成功或失败均缓存到设置改变，停止同步删除自己的条目，文件空了就删文件。不用系统钥匙串：没有固定签名身份的构建（未签名或临时签名）每次重新编译都会让钥匙串重新弹授权框；代价是备份或拷走整个仓库文件夹时凭据会跟着走。
- **触发时机**：
  - 本地改动后 10 秒内没有新改动就推送；
  - 每 5 分钟拉取一次；
  - 启动、从睡眠唤醒、手动点击"立即同步"时各同步一次；
  - 唤醒用每秒墙钟采样检测（相邻样本间隔超过 15 秒）；
  - 单工作线程串行同步，运行中再次触发会接着再跑一次。
- 同步文件夹由同步盘传递文件；其余后端直接联网并使用 reqwest 的系统代理支持。**按流量计费暂停已实现**：网络后端默认启用，macOS 用 Network.framework 路径监视器读取 isExpensive / isConstrained；Windows 用 Windows.Networking.Connectivity 的连接费用 API（每秒检测，已通过 Windows 编译，系统计费状态切换待人工验收）。检测失败按非计费处理。计费时跳过自动同步，手动“立即同步”可跑一轮；网络恢复后立即同步。调试版支持 `STICKER_FORCE_METERED=1`，发布版忽略。
- **加入新设备**：在偏好设置的“同步”分组选择同一存储目标；网络连接先探测读、写、删除权限，已有仓库显示设备数、表情数并确认加入。设备名默认用电脑名，可修改，保存在本机 `local/device.json`。分组支持取消连接、重新输入凭据、上传/下载原图进度；标题栏显示同步状态，左栏卡片显示后端名和简短地址。
- 手动暂停保存在本机设置，重启保留；暂停后取消当前轮次，保留上次同步时间与设备列表，所有自动触发跳过。继续同步立即跑一轮。设备列表和上次同步时间缓存于本机 SQLite 的 `sync_device_cache`；0003 迁移增加每张表情的创建来源，用设备 received 计算实际拥有该表情的设备数。
- **日志维护**：
  - 本机：已推送的 `sync_stream` 行，以及已折叠进 `sync_model`、当前同步目标也已分配序号的 `sync_outbox` 行，在推送和接收的事务里顺带删除；没开启同步时也会折叠和清理。运行中只复用空页，空页够多时才在启动、窗口出现之前 VACUUM。3000 张、真实同步过的仓库上，数据库从 11.6 MB 降到 5.0 MB，清理持锁 18.5 ms，启动 VACUUM 29.6 ms。
  - 远端：同步成功后，本机日志目录里不到 4 MiB 的段攒够 32 个，就把连续的一组（每轮最多 128 个）合成一个段，保留全部历史和格式 1。网络后端在按流量计费的网络上不合并，手动立即同步也一样。
  - 完整条件、中断后的恢复和请求预算见 [ADR 0005](adr/0005-sync-format-and-merge.md)。
- 第一版不做端到端加密。

## 默认设置

- 全局快捷键：macOS ⌃⌥E，Windows Ctrl+Alt+E，在鼠标所在的屏幕弹出面板。
- 开机自启：发布版首次从稳定位置启动时默认开启，调试版不自动开启。macOS 排除 `/Volumes/`、App Translocation、系统临时目录和当前 `TMPDIR`；这些位置既不登记，也不设置首次初始化标记，不覆盖原有登记。之后若系统登记仍开启、当前位置稳定，但 LaunchAgent 的可执行文件路径与当前规范化路径不同，则重新登记；用户关闭后不自动重开。手动开关仍直接按请求执行，包括不稳定位置。插件没有读取登记路径的接口，`autostart.rs` 用 `plist` 读取 `~/Library/LaunchAgents/<应用名>.plist`。自启参数为 `--autostart`，只显示悬浮球和托盘。Windows 保持原有行为。
- 悬浮球：开启，初始在主显示器右边、高度 40% 处；静止 3 秒后收起，可改为 10 秒或不收起。

## 菜单栏

- macOS 由 `src-tauri/src/menu.rs` 提供“表情管理器 / 编辑 / 窗口”三组中文菜单，Windows 不添加菜单栏。关于只提供配置中的名称和版本号。
- AppKit 会追加 AutoFill、听写、表情符号和 Close All；编辑与窗口子菜单在打开时只保留指定的原生动作及分隔线，不修改系统偏好设置。
- 偏好设置（⌘,）和退出（⌘Q）共用托盘的 `entry-preferences` / `entry-quit` 分发；macOS 桌面版不再重复监听网页里的 ⌘,。编辑菜单保留原生 responder chain，文字编辑和表情快捷键沿用原有处理；关闭窗口（⌘W）走 `CloseRequested`，与红色关闭按钮一样只隐藏。

## 发布

- macOS 最低版本为 14.0，设置在 `bundle.macOS.minimumSystemVersion`。启动错误对话框调用的 `NSApplication::activate` 要求 macOS 14；Network.framework 路径监视及费用检测最高要求 10.15，当前 NSPanel 用法不要求更高版本。按 API 要求设定，未在旧系统上运行。打包与隔离 HOME 的试运行方法见[打包说明](packaging.md)。
- 没有 Apple 开发者签名和公证：`bundle.macOS.signingIdentity` 设为 `"-"`，打包时对整个 .app 做临时签名（ad-hoc），macOS 第一次打开时要在"系统设置 › 隐私与安全性"里点"仍要打开"。只有链接器自带的签名、资源没有封装进签名时，从网上下载的包会提示"已损坏"，而且没有放行按钮。Windows 未签名，会被 SmartScreen 拦一下，点"仍要运行"即可。
- 公开发布前再补上：Apple 开发者账号签名和公证、Windows 代码签名证书。

## 远期设想

还没决定做，先记下来。已经决定要做的在 [TODO.md](TODO.md)。


- 用大模型自动识别配文：只在表情还没有配文时自动填入，不覆盖手动填写的。
- 端到端加密（可选开关）。
- 移动仓库位置。
- 支持视频表情。
- 点击表情后自动粘贴到当前焦点输入框（[#15](https://github.com/wjiajian/sticker-manager/issues/15)）：复制并收起面板后模拟 ⌘V / Ctrl+V。2026-11-10 评估暂不做：只省一次粘贴键，拖出已能直接放进输入框；macOS 需要辅助功能权限，临时签名下授权会随更新失效；无法判断焦点是否为输入框（访达里会复制出文件）；固定面板时焦点留在面板上。完成 Apple 签名或有用户反馈后再评估。
