# 深色外观、清空最近删除、日志维护与剪贴板探索记录

> 历史记录：代理交回的实现与验收报告，对应提交 8a79087。内容按原文保留，只把表格恢复成 Markdown、链接改成仓库内相对路径。审查后改动过其中部分内容：浅色配色恢复原值，清空最近删除改为不受类型筛选影响，远端合并补了半截文件的恢复。现状以 [design/README.md](../../design/README.md)、[docs/tech-stack.md](../tech-stack.md)、[ADR 0005](../adr/0005-sync-format-and-merge.md) 和 git 历史为准。

已停止后续实测。前三项已实现，剪贴板产品行为未改，未执行 git commit。临时测试进程已结束，原剪贴板已恢复，没有发送消息。

- 深色模式：新增跟随系统／浅色／深色，统一主窗口、隐藏面板、预览、悬浮球及原生菜单、对话框。图片区采用中灰，兼顾黑色和浅色描边；配色与对比度已写入设计文档。
- 清空最近删除：使用确认时的 ID 列表，在事务内跳过已恢复的表情，保留后来新删除的表情；完成后焦点移到列表标题。2000 张连同文件删除持锁 143.7 ms。
- 日志维护：清理已处理的本地日志，启动时按阈值回收数据库空间；远端每轮最多合并一组本机日志，保留全部历史。支持中断恢复，计费网络上不合并。

## 数据库大小

3000 张真实同步仓库的数据库从 11,612,160 B 降至 4,964,352 B，减少 57.2%。清理持锁 18.5 ms，启动 VACUUM 29.6 ms；另测关闭同步后的 3000 条修改折叠为 124.6 ms。

| 占用（KiB） | 清理前 | 清理后 |
|---|---|---|
| sync_stream | 3768 | 4 |
| sync_outbox | 2412 | 4 |
| sync_model | 2636 | 2636 |
| stickers | 716 | 504 |
| sync_origins | 360 | 360 |
| usages | 268 | 268 |
| 全部索引合计 | 1112 | 1004 |

tags、tag_groups、sticker_tags、sync_usage_offsets、sync_targets、sync_progress、sync_pending、sync_meta、sync_device_cache、sync_deleted_objects 均为每表 4 → 4 KiB；SQLite 内部表未变。

## 验证

格式检查、151 项单元测试、Clippy、TypeScript 检查、前端构建全部通过；Docker WebDAV／MinIO 的 6 项集成测试全部通过，包括日志合并与断线恢复。

## 剪贴板探索

剪贴板保留了 146 次粘贴记录：Chrome、文本编辑、备忘录各 40 次，Safari 26 次。下表按 PNG／JPG／GIF／WebP 四种输入概括；“静”表示采样时只见静态画面，“插”表示确认插入，但呈现类型未核实。

| 组合 | Chrome | 文本编辑 | 备忘录 | Safari |
|---|---|---|---|---|
| PNG | 静×4 | 静×4 | 插×4 | 静×4 |
| 文件 URL | 静×4 | 无反应×4 | 插×4 | 无反应×4 |
| 文件 URL＋PNG（现状） | 静×4 | 静×4 | 插×4 | 无反应×4 |
| GIF＋PNG | 静×4 | 静×4 | 插×4 | 静×4 |
| GIF＋PNG＋文件 URL | 静×4 | 静×4 | 插×4 | 无反应×4 |
| TIFF＋PNG | 静×4 | 静×4 | 插×4 | 静×4 |
| 图片、文件拆成两条 | 静×4 | 静×4 | 插×4 | PNG/JPG 无反应，其余未完成 |
| PNG＋文件 URL（反序） | 静×4 | 静×4 | 插×4 | 未测 |
| HTML 图片＋PNG | GIF 动；WebP 动画源保留，播放未确认 | 静×4 | 插×4 | 未测 |
| GIF＋PNG＋public.url | 静×4 | 静×4 | 插×4 | 未测 |

Chrome 的 PNG 透明度已确认保留；Safari 的纯 PNG 组合也已确认。文本编辑的 WebP 样本较小，动画判断有限；备忘录的窗口截图未成功取得，因此没有把插入成功当作动画或透明度验证。

现有证据倾向于：静态图片的“复制”只写 PNG，“复制为文件”只写文件 URL。动图策略和按应用分流，留到实际遇到问题时再决定。非激活面板可读取前台应用的 bundle ID；主窗口复制时前台是本应用，且复制时的应用未必是真正粘贴目标。

## 改动位置

主要改动位于[外观模块](../../src-tauri/src/appearance.rs)、[主题变量](../../src/styles/global.css)、[删除逻辑](../../crates/core/src/deletion.rs)、[列表操作](../../src/windows/main/useStickerActions.ts)、[本地日志存储](../../crates/core/src/sync/store.rs)、[远端合并](../../crates/core/src/sync/compaction.rs)，以及相关窗口、组件、IPC 和测试。设计、技术栈、ADR 0005、TODO、术语表已同步更新。

截图在代理本机：`~/.codex/visualizations/2026/11/02/01a19b79-a16e-7a21-bbd3-60a6413d31bb/` 下的 `sticker-validation`（深浅色界面）和 `clipboard-validation`（剪贴板实验），没有放进仓库。

## 没测的

悬浮球悬停、原生菜单取消、部分窗口初始化和原生应用粘贴曾短暂接管前台，随后还原。系统外观自动切换、Windows、真实拖拽未测；QQ、飞书、预览、邮件、信息的剪贴板实验按用户的最新要求停止，微信按用户的选择跳过。
