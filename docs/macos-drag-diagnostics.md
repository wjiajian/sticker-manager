# macOS 拖拽类型诊断

Issue [#7](https://github.com/wjiajian/sticker-manager/issues/7) 报告 macOS 上微信和飞书表情无法拖入悬浮球，但没有记录拖拽所提供的数据类型。原有 Tauri/Wry 回调只提供文件路径；悬浮球现另接收标准图片数据和文件承诺。具体来源是否使用这些表示、能否成功导入，仍需 macOS 真实来源验证。

在 macOS 的仓库根目录运行：

```sh
swift scripts/diagnose-macos-drag.swift
```

分别从微信、飞书和 QQ 拖一张表情到诊断窗口，松手后记录终端输出，并注明来源应用及其版本、macOS 版本，以及同一表情拖到悬浮球时的结果。关闭诊断窗口会结束程序。没有出现「进入」输出时也请记录；诊断窗口只注册了常见的文件、图片和文件承诺类型，不保证能收到只提供私有类型的拖拽。

脚本只输出拖拽操作掩码和 pasteboard 类型名称，不读取文件名、图片内容或请求源应用生成承诺文件，也不会访问表情仓库。分享输出前请自行检查类型名称是否包含不希望公开的信息。诊断窗口与应用使用的 WKWebView 不同，结果只说明该次拖拽向这个窗口提供了什么，不能代替在应用中的端到端验证。

文件 URL、图片数据和文件承诺是不同的拖拽表示。目前支持常见 PNG、JPEG、GIF、WebP 和 TIFF 图片数据，以及系统文件承诺；只提供私有格式的来源未必适用。完成后仍需在 macOS 上验证微信、飞书、QQ 及 Finder 文件拖入。Apple 的[拖拽目标文档](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/DragandDrop/Concepts/dragdestination.html)说明必须从当次拖拽获取 pasteboard；[文件承诺示例](https://developer.apple.com/documentation/appkit/supporting-drag-and-drop-through-file-promises)说明承诺文件需异步接收。
