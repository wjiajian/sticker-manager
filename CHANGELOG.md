# 变更日志

本项目按用户可见行为和兼容性记录变更。日期采用 `YYYY-MM-DD`；测试条目只记录已经执行或明确标注为待执行的内容。

## 2026-09-16 macOS 工程与持续集成

### 新增

- macOS 应用图标与菜单栏图标复用现有 Windows ICO 中的 Flutter 默认图案。菜单栏直接读取 ICO；仅 macOS 构建通过 Xcode scheme 调用系统 `sips` 生成应用图标 PNG，生成文件不纳入 Git。Windows/Android 构建恢复原有流程，Flutter 测试无需准备图标，图标生成不再依赖 Node.js。
- 使用 Flutter 3.47.3 官方模板创建 macOS 工程，最低系统版本为 macOS 12，包含沙箱文件选择权限与原生剪贴板方法通道。
- macOS 提供菜单栏图标、可配置的 `Cmd+Shift+E` 快捷键、快速选择窗口、关闭隐藏及 Dock 恢复、右键管理和鼠标框选。
- 静态图片复制为 PNG，GIF 保留原始数据和文件 URL。成功复制计入使用次数，用户切换目标应用后按 `Cmd+V` 粘贴；macOS 不模拟粘贴或回车。
- Actions 在推送、PR 和手动触发时分析、测试并构建 Windows、macOS、Android，上传桌面 ZIP 和测试 APK。macOS ZIP 使用 `ditto` 保留应用包结构，工作流包含原生剪贴板 XCTest。
- 数据库 schema 和迁移包格式保持不变，Windows 专属目录探测与自动发送保持原有行为。

### 已执行验证

- Flutter 3.47.3：`flutter analyze --no-pub` 通过，`flutter test --no-pub` 31 项通过。
- 自定义 Swift 桥接使用 Flutter SDK 头文件完成类型检查；工程 plist、entitlements、Podfile、构建脚本和工作流 YAML 语法检查通过。
- `node tool/verify_project.mjs` 和 `git diff --check` 通过。

### 待验证

- 本机仅有 Command Line Tools，`flutter build macos --release --no-pub` 因缺少完整 Xcode 未执行编译。原生 XCTest、完整应用构建和菜单栏、快捷键、QQ/微信静态图与 GIF 粘贴仍需在完整 macOS 开发环境验证。
- Actions 配置尚未在 GitHub 执行；CI 产物未配置 Developer ID 签名、公证或 Android 正式签名，用于测试。

## 2026-09-15 发布前修复与验证

### 修复

- Windows 发送目标在原生 `WM_HOTKEY` 和托盘按下阶段捕获，并校验 HWND 所属进程、窗口类名和标题；目标失效时只在原进程内有限重解析，避免把粘贴和回车发送到错误窗口。
- Windows 重复启动只激活现有实例并恢复主管理模式，不再产生第二个项目进程、托盘图标或全局热键。
- Windows/Android 导入增加单文件 `64 MiB`、批次 `512 MiB` 上限；分享 URI 使用有界流读取，超限和读取失败会在应用中提示。
- 加密迁移包增加密文 `768 MiB`、明文 `512 MiB` 上限，以及 manifest、媒体类型、哈希、分组 ID 和路径校验；导入完成后释放已处理归档内容。
- 发布脚本优先读取 `flutter config --machine` 中的 JDK 和 Android SDK。此次 Android 构建使用 JDK 17.0.20.1，不依赖系统 Java 11。
- Windows ZIP 根目录增加 `README_FIRST.txt` 和 `Start-StickerManager.cmd`，解压后可以按说明直接启动，无需安装程序或管理员权限。

### 构建产物

- Windows x64 便携包：`dist/sticker-manager-windows-x64-0.1.0+1.zip`
  - SHA-256：`FFCC7732A53BFF7F8C34983A8981962F865CEF22C7DB875AF94483ABDBA7C339`
- Android APK：`build/app/outputs/flutter-apk/app-release.apk`
  - SHA-256：`DDE2ABDD0CE5D1DEC6777EC50BCB1690745B6EBEFAE38FBFA8324BD3F7294C21`
  - `minSdkVersion 28`、`targetSdkVersion 36`，包含 `arm64-v8a`、`armeabi-v7a` 和 `x86_64`。
  - 当前使用 Android Debug 证书，仅用于测试；`com.example.sticker_manager` 仍是示例 applicationId，公开分发前必须更换签名和 applicationId。

### 已执行验证

- `flutter analyze --no-pub`：通过。
- `flutter test --no-pub`：28 项通过。
- `flutter build windows --release --no-pub`：通过。
- `flutter build apk --release --target-platform android-arm64 --no-pub`：通过。
- Windows 单实例并发启动检查：最终只保留 1 个项目进程。
- Windows 便携包内容检查：未包含临时截图、数据库、日志或密钥文件。
- Android APK v2 签名和 ABI/min SDK 检查：通过。

### 尚待设备复测

- QQNT、传统 QQ 和微信桌面版的真实窗口焦点、静态图/GIF 剪贴板、粘贴和回车发送链路仍需在用户设备逐一复测。
- Android 分享接收、悬浮窗权限拒绝/恢复、旋转和 GIF 长按预览尚未进行真机验收。
- 当前 APK 不适合公开分发；正式发布还需要配置独立签名密钥并更新 applicationId。

## 0.1.0+1 交互、窗口与留痕整理

### 新增

- 建立中文项目规格、分层架构和架构决策记录，约定后续行为与格式变更的留痕要求。
- 规划 Windows 卡片右键管理菜单、紧凑快速选择窗口和标准框选语义：普通拖拽替换、`Ctrl` 追加、`Esc`/顶部关闭/空白右键退出；多选时支持滚轮浏览。

### 修复与调整

- 托盘右键显式弹出“打开表情管家”和“退出”菜单；退出路径清理热键和托盘资源。
- 备注搜索继续覆盖备注、导入文件名备注和表情 ID，并区分“没有表情”和“搜索无结果”。
- 快速选择模式与主管理模式分离；快速模式默认约 `760×600`，使用高密度网格，保留系统最大化和还原。
- 管理动作移入 Windows 卡片右键菜单，置顶文案随状态切换；单击仍专用于复制/粘贴/发送。
- 多选框选改用原始指针事件，避免与网格滚动手势竞争；普通模式保留滚动，多选模式可用滚轮浏览。
- 右键菜单打开主管理窗口时清除一次性发送目标；没有目标时只复制且不增加 Windows 使用次数，避免误发到历史窗口。
- 混合选择 `Tencent Files` 父目录时，收到/市场表情按文件默认取消勾选，仍可由用户明确选择导入。
- 修复目标窗口竞态：Windows runner 在 `WM_HOTKEY` 阶段先保存原生前台 HWND 快照，并冻结目标 PID、窗口类名和标题。
- QQNT HWND 被重建时，只在原 PID 内按冻结元数据重解析可见顶层窗口；激活经过短暂重试并验证实际前台句柄后，才允许发送粘贴和回车。
- 收紧 QQNT 窗口重解析：标题变化时只有同类窗口唯一才允许替代，避免把发送按键注入同进程的其他窗口。
- 增加有效 HWND 的类名和标题身份校验，降低同进程 HWND 复用后误发送到主壳窗口的风险。
- 右键菜单打开主管理窗口会清除旧的一次性发送目标；没有有效目标时只复制到剪贴板，不误发到历史窗口。
- 主管理界面的“快速唤出”按钮和托盘图标入口可在 5 秒内复用最近外部窗口快照；快照过期时仍只复制，避免误发。
- 托盘图标左键/双击打开完整主管理窗口时捕获外部目标，完整窗口卡片可直接粘贴并发送；双击事件合并为一次，右键菜单“打开表情管家”仍保持纯管理行为。
- 忽略托盘任务栏和桌面壳窗口的前台事件，避免托盘点击时把 `Shell_TrayWnd` 误当成发送目标。

### 验收关注点

- QQNT 个人表情目录的批次和文件级导入选择，QQ 收藏源顺序，静态图/GIF 剪贴板。
- 热键与托盘完整窗口的目标捕获、右键菜单管理入口无目标时的“只复制”、单击/双击去重和回车发送结果。
- 真实 Windows 托盘、QQNT/QQ 桌面版/微信桌面版以及 Android 分享、悬浮面板和 GIF 长按预览。

### 2026-09-13 验证记录

- `flutter analyze --no-pub`：通过。
- `flutter test --no-pub`：28 项通过。
- `flutter build windows --release --no-pub`：通过。
- `flutter build apk --release --target-platform android-arm64 --no-pub`：通过；当前未配置独立签名密钥，APK 使用 debug key，仅适合测试。
- Windows 单实例检查：3 次并发启动最终保留 1 个项目进程。
- Windows 端到端：使用当前用户保存的 `Ctrl+↑` 热键，以记事本和 QQNT 为目标，快速面板选择后均恢复目标窗口并记录 `sent`。网格需完成首批加载后再按回车选择。
- 当前用户偏好中的热键是 `Ctrl+↑`；新配置默认仍为 `Ctrl+Shift+E`，两者不一致时应以设置页显示为准。
- 托盘左键按下阶段已加入 runner 原生快照，完整主管理窗口的单击发送验证待使用该最新 Release 包完成；QQ/微信真实兼容性仍需在目标设备复测。

## 0.1.0+1 - 首版本地库

### 新增

- Flutter Windows/Android 共享 UI 和本地 SQLite 数据层。
- 图片/GIF 媒体托管、SHA-256 去重、分组、备注、置顶、使用频率排序和 `QQ收藏` 分组。
- Windows 托盘、`Ctrl+Shift+E` 全局热键、目标窗口捕获、Win32 图片/GIF 剪贴板和原生粘贴/回车。
- Android `ACTION_SEND`/`ACTION_SEND_MULTIPLE` 分享导入、文件选择、系统剪贴板和可选悬浮面板。
- 版本化加密迁移包：manifest、媒体、可选缩略图和 AES-GCM 认证加密。

### 导入规则

- QQNT 自动探测限定纯数字账号下的 `nt_qq\nt_data\Emoji\personal_emoji\Ori`，并识别旧版 QQ 的命名表情目录。
- 使用文件签名识别图片，支持无扩展名文件；单个明确目录最多扫描 1,000 个文件。
- 自动导入先预览来源、数量和重复项，用户可逐文件确认；不修改 QQ/微信原文件。

### 性能与可靠性

- 导入分为有限并发准备、批量事务提交和后台缩略图生成；记录提交后即可显示网格。
- 缩略图版本迁移使用 Flutter 原生解码器，损坏媒体保留原文件并回退到原图或占位。
- Windows runner 使用单实例互斥体；Android 分享队列在确认前持久化，悬浮服务启动具备幂等保护。

### 已验证基线

- Flutter 单元测试：28 项通过（模型、排序与搜索、数据库、导入、缩略图、迁移包、偏好设置、剪贴板结果语义）。
- `flutter analyze`：无问题。
- Windows Release 和 Android arm64 Release 构建已成功生成；真实 QQ/微信兼容性仍需在目标设备上复测。

### 已知限制

- Windows 原生剪贴板格式、窗口焦点和托盘行为无法完全由 Flutter 单元测试覆盖，需要真实桌面应用验收。
- Android 悬浮面板受系统厂商权限策略影响；分享来源应用必须提供可读取的 `content://` URI。
- 没有配置独立签名密钥时，Android release 构建使用项目本地 debug 签名，只适合测试，不适合公开分发。

## 维护记录格式

后续每次发布或行为变更至少记录：用户可见变化、数据/兼容性影响、测试命令或手工验证范围、尚未覆盖的限制。迁移格式或导入路径变更还必须同步 `docs/DECISIONS.md`。
