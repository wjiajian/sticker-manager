# Windows 构建与验收

## 代码审查修复与构建流程调整

2026-11-04，按用户要求移除原“Windows 构建与平台回归”的 push 触发和 macOS 回归作业，改为独立、手动触发的“构建 Windows 安装包”。原 `build-macos.yml` 和 macOS 更新清单脚本未修改。Windows 默认流程与 macOS 对齐：安装依赖、类型检查、版本校验、构建、可选签名、上传产物；勾选 `release` 才发布，`validate` 可额外运行回归检查。Windows 更新使用 `latest-windows.json`，不覆盖 macOS 清单。

| 审查项 | 处理 |
|---|---|
| 1、3、5：仓库迁移 | 空当前目录不遮住更早仓库；Windows 通过跨进程文件锁串行化迁移；元数据和目录读取错误向上传递，避免打开空仓库。补充空目录、并发启动、旧路径不是目录的回归测试。 |
| 2：总路径过长 | 只有存在非法或超长组件时才把对应名称错误视作旧路径不存在；合法组件组成的总路径过长仍报错，读、删、路径检查使用同一判断。 |
| 4：控制台保留名 | 拒绝 `CONIN$`／`CONOUT$`，测试覆盖写入前拒绝、同步日志尚未生成以及旧日志改名恢复。 |
| 6：慢拖入来源 | 数据提取与流读取转入 COM 后台线程，通过 `AgileReference` 封送并保留对象。支持异步的来源配对调用 `StartOperation`／`EndOperation`；旧同步来源保留 Drop 生命周期，以 OLE modal wait 派发窗口消息。补充慢源、原引用释放、失败通知及同步来源后台读取测试。 |
| 7：拖入登记失败 | 自定义拖入登记为可选能力，失败时撤销已登记目标并记录日志，继续创建应用。 |
| 8：自动触发范围 | 与用户本次明确要求冲突，因此不增加 push／PR 触发；改为两个独立手动构建，不再依赖特性分支自动回归。 |
| 9：IStream 覆盖 | mock 同时匹配格式、索引和 `tymed`，新增仅提供 IStream 的虚拟文件完整接收测试，错误请求 HGLOBAL 会失败。 |
| 10：路径文档 | 更新 ADR、README 和打包说明，记录安装版路径、迁移顺序和便携版例外。 |

OLE 生命周期依据：[异步 Shell 数据传输](https://learn.microsoft.com/en-us/windows/win32/shell/datascenarios#dragging-and-dropping-shell-objects-asynchronously)、[等待期间派发窗口消息](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/ne-combaseapi-cowait_flags)。本轮没有操作真实仓库、修改系统设置或执行真实拖拽；此前用户验收属于下述旧构建，不替代本轮自动回归。

验证记录：应用提交 `2fae5c5` 的 [Windows 手动构建](https://github.com/wjiajian/sticker-manager/actions/runs/41841578684) 已通过类型检查、格式、Clippy、全量 Rust 测试和更新清单脚本测试，成功生成带更新签名的 NSIS、便携 ZIP 和 `latest-windows.json`。原 [macOS 工作流](https://github.com/wjiajian/sticker-manager/actions/runs/41840837983) 在 `833bb06` 上通过类型检查并生成 DMG、更新包和 macOS 清单；其后代码修改仅涉及 Windows 分支及测试。两次运行均设置 `release=false`，没有创建或修改 Release。

Windows 产物已下载到 `target/windows-review-artifacts/`：`sticker-manager_0.1.5_x64-setup.exe`（7,645,131 字节）、`sticker-manager_0.1.5_windows-x64-portable.zip`（11,213,517 字节）及独立更新清单。已检查 ZIP 包含 EXE、`portable.flag`、使用说明，清单仅包含 `windows-x86_64`，安装包 URL 与 ASCII 文件名对应。后续收尾提交只调整文档和工作流运行标题，构建步骤未变。

## 六项反馈修复（此前已验收产物）

2026-11-04，用户确认本轮六项反馈均已完成实际测试验收，包括微信／QQ 等聊天软件拖入。下表保留代理执行检查时的证据和范围；其中实际拖拽及任务栏外观现已由用户验收补齐。

2026-11-04，应用提交 `607817288a412a9c80c1be1d4aa2a925536fbce1`。[双平台 CI](https://github.com/wjiajian/sticker-manager/actions/runs/41836309925) 全部通过：Windows 203 项测试、macOS 195 项测试，各有原有 8 项网络／容器测试忽略；两边的类型检查、前端构建、格式检查、Clippy 和应用构建通过。版本仍为 0.1.5。

| 反馈 | 实现与验证 |
|---|---|
| Windows 仓库路径 | 安装模式使用 `%APPDATA%\sticker-manager`，目标为空时自动迁移现有标识符目录，并兼容更早的目录。临时目录测试验证优先级与已有数据保护；没有操作用户真实 AppData 仓库。 |
| 便携版 | ZIP 内含 EXE、`portable.flag` 与说明，仓库为旁边的 `data`，WebView 缓存为 `webview`。实际 Release 程序验证空仓库启动、导入一张测试图、整目录搬迁后重启，数据与缩略图保留；自启请求被拒绝，更新入口提示手动替换程序。需要已有 WebView2 Runtime。 |
| 悬停预览关闭面板 | Windows 预览改为不激活窗口的原生显示／隐藏，悬浮球穿透样式更新不再触发激活。实际 Debug 实例在未固定状态连续验证 5 次预览，预览显示、图片解码成功，面板始终可见并保持焦点，预览不获得焦点。 |
| 左上角标题 | 仅 Windows 图标由 24 增至 32 CSS 像素，标题由 14 增至 16；真实程序截图与 960×600 布局检查通过。 |
| 任务栏图标 | Windows 专用 ICO／PNG 减少图案留白，主窗口与打包配置使用新图标，macOS 原图标未修改。已检查图标资源及构建配置；未截取用户整个任务栏作视觉验收。 |
| 聊天软件拖入 | Windows 悬浮球登记原生 OLE 接收器，支持本地文件、虚拟文件／文件流、PNG、DIB／DIBV5，复用现有导入和结果提示。实际程序初始化成功；5 项数据格式测试通过，覆盖源文件保留、虚拟 GIF 字节保留、流分块、PNG、位图转换、异常数据与临时文件清理。按项目约定没有进行微信／QQ 的真实拖拽，具体客户端仍以用户使用反馈为准。 |

测试均使用 `D:\Environment\sticker-manager-tests\followup-6fbd34c947d24ea7bdb6de6ef3de1a0d` 下的临时数据，并设置绝对路径 `STICKER_LIBRARY_DIR`。Release 便携版通过标记文件实际隔离数据。窗口检查通过指定 WebView 后台执行；为验证焦点短暂显示了测试面板，没有移动用户鼠标或接管整个屏幕。测试实例均已关闭。强制终止后 WebView 缓存曾短暂被子进程占用，等待其退出后整目录搬迁成功，日常移动前应从菜单正常退出。

当前产物（已替换工作区同名旧构建）：

| 产物 | 路径 | SHA-256 |
|---|---|---|
| Debug | `target/x86_64-pc-windows-msvc/debug/sticker-manager.exe` | `98af50bb1a8d3faed69ebe363ba5e634b1dba5f7aad959890c50d5011ccf5560` |
| NSIS | `target/x86_64-pc-windows-msvc/release/bundle/nsis/表情管理器_0.1.5_x64-setup.exe` | `0b82b4379fe5daa591fc3c7f130908bf91c30dc87f9845ef9fc0f28c53c10c14` |
| 便携 ZIP | `target/portable/sticker-manager-windows-x64-portable.zip` | `c8cc8b9ed0283cfd5302886f14115f93b2e2f33a50a184fb041ae6e5c56fe416` |

验收记录与截图位于 `target/windows-validation/followup-results.json`、`followup-main.png`、`followup-preview.png`。本轮新增依赖只在 Windows 目标启用；锁文件只增加已有版本 `windows-core 0.62.2` 的直接依赖关系，没有升级依赖版本。macOS 保持原仓库路径、窗口分支、图标和发布流程；本轮未执行 macOS 真实窗口验收。

## 初次 Windows 构建（历史记录）

以下记录对应旧提交，产物已由上面的当前构建替换。

2026-11-04 完成 Windows x64 构建与隔离运行验收。应用代码为 `2ed0317179786eb07f2b8d3031a3a54f8bfeac50`，分支为 `codex/windows-platform`，后续文档提交沿用这份构建产物。

## 初始情况与实现范围

项目原有 Windows 剪贴板、托盘、悬浮球、面板和网络费用检测分支，但缺少实际编译记录与 Windows 主窗口控制。此次复用现有后端与界面，仅补充 Windows 标题栏及权限、双平台构建工作流和验收中发现的 Windows 问题。

- Windows 主窗口使用应用图标、名称及最小化、最大化／还原、关闭按钮，关闭继续隐藏窗口。
- WebView2 在 setup 结束前可能查询同步状态，Windows 提前登记同步状态，避免启动竞争。
- Windows 分数缩放会把 216 CSS 像素测为约 215.975，预览回执在 Windows 取整后交给原生校验。
- Windows 同步文件夹在写入移动日志前检查文件名，并支持改名后重新规划旧客户端留下的非法目标。读取和清理可兼容外部程序已创建的保留名路径。
- macOS 的窗口创建、启动顺序、预览尺寸和发布工作流沿用现有路径。共享存储校验钩子默认通过，Windows 本地文件夹实现具体限制。

## 构建结果

Windows 与 macOS 均通过前端类型检查、生产前端构建、Rust 格式、Clippy 和测试。Windows 生成内嵌前端的 Debug 程序与 NSIS 安装包，macOS 生成 `.app`。Windows 测试通过 196 项，macOS 通过 195 项，两边各有 8 项需要网络／容器的测试按原设置忽略。六项 Windows 专用回归测试全部通过，覆盖非法与超长名称、保留名旧文件、Unicode、新移动意图和旧意图恢复。[CI 运行记录](https://github.com/wjiajian/sticker-manager/actions/runs/41825705940)

版本保持 `0.1.5`，依赖和锁文件沿用原版本。安装包未签名，正式发布、自动更新与 MSI 留待后续安排。

| 产物 | 工作区路径 | 字节数 |
|---|---|---|
| Debug 程序 | `target/x86_64-pc-windows-msvc/debug/sticker-manager.exe` | 44796928 |
| NSIS 安装包 | `target/x86_64-pc-windows-msvc/release/bundle/nsis/表情管理器_0.1.5_x64-setup.exe` | 7535315 |

SHA-256 分别为

```text
Debug  9fd9f95d0942da52ef3c3af1bc008d52b817369252f4fa586c502afd56dc0c12
NSIS   dbed696904d6d27172c7b0ad6a7f5dfa8cf89f9d9929a7d3011a87b687c40160
```

## 本机环境与隔离

- Windows 11 专业版，系统构建 26300，屏幕 2560×1440，应用报告的缩放约为 1.23958。仅覆盖这一台机器和当前缩放。
- 所有新增本地工具及可配置缓存置于 `D:\Environment`，环境脚本为 `D:\Environment\sticker-manager-env.ps1`。复用现有 Node 和 WebView2，Rust／pnpm／便携 GitHub CLI 使用 D 盘路径。MSVC 和 SDK 构建由 GitHub runner 执行。
- GitHub CLI 凭据以明文配置保存在 `D:\Environment\gh\config\hosts.yml`，目录和配置文件取消继承，仅当前 Windows 用户有访问权限。该方式用于满足认证配置位于 D 盘的要求。
- 每个测试实例都设置绝对路径 `STICKER_LIBRARY_DIR`，仓库、同步目录、WebView 数据和临时导出文件位于 `D:\Environment\sticker-manager-tests`。测试结束已关闭实例，剪贴板检查后恢复原有内容。

## 运行验收

通过真实 Debug 程序的界面事件、原生命令和剪贴板读取验证。后台可完成的界面操作通过指定 WebView 执行。

| 检查项 | 结果与范围 |
|---|---|
| 启动 | 修复后正常启动，三次独立 WebView 配置的 `--autostart` 启动均加载四个页面，主窗口隐藏，读取 65 张测试表情，日志无 panic |
| 标题栏 | 原生最小化与重开、最大化与还原、关闭后进程保留通过，960×600 布局检查通过 |
| 导入 | 原生文件选择器导入 GIF 通过，文件夹 60 张、PNG／JPG／WebP／GIF／APNG 与 ZIP 去重通过实际导入命令验证，全部 65 张图片可加载 |
| 整理 | 搜索、配文、收藏、新建分类、设分类、删除及撤销通过 |
| 剪贴板 | 普通复制包含 PNG／CF_DIB／CF_HDROP，PNG alpha 范围 0–255，Ctrl+Shift+C 为文件引用，GIF 原文件哈希一致 |
| 面板 | 实际 Ctrl+Alt+E 呼出、固定后复制成功并保持显示、固定失焦保留、取消固定失焦隐藏、Esc 收起、滚动加载第 49–65 张通过 |
| 预览与悬浮球 | 当前分数缩放下悬停预览可显示并解码图片，应用设置可隐藏和显示悬浮球 |
| 外观 | 应用内深浅切换可更新四个窗口，检查后恢复跟随系统 |
| 同步文件夹 | 非法名称给出可操作错误，新增意图数量保持零，改名后同步成功，旧错误意图也成功恢复 |
| 原生菜单退出 | 用户手动点击测试实例菜单中的退出并回复确认，随后核实进程已结束 |

原生文件／文件夹选择器、全局快捷键与焦点切换测试短暂使用前台。焦点测试仅在测试应用的主窗口和面板之间切换。原生菜单退出由用户操作。拖入、拖出、拖回和窗口拖动均按项目约定只做代码检查。

## 留待人工验收的范围

- NSIS 安装、卸载、发布版首次运行及开机自启。当前运行验收使用支持临时仓库变量的 Debug 程序。
- 微信、QQ、飞书中的实际粘贴、透明显示与发送后 GIF 动画。
- 原生文件夹选择器的最终确认。工具可打开并导航到测试目录，完成操作受工具限制，文件夹批量导入已通过实际命令验证。
- 系统计费状态、减少动态效果、系统深浅切换及多屏／其他 DPI 组合。此次保持系统设置。
- macOS 真实窗口回归。此次保护措施包括平台条件分支、现有路径核对及 macOS CI 全套检查与应用构建。

本地详细记录、截图、CI 日志及产物清单位于工作区 `target/windows-validation/`，均为忽略的验收文件。
