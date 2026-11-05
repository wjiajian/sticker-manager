---
title: 安装
description: 在 macOS 和 Windows 上下载并安装表情管理器，第一次打开时怎样放行。
---

表情管理器支持 macOS 14 及以上的 Apple 芯片电脑，以及 Windows x64。所有安装包都在 [GitHub 发布页](https://github.com/wjiajian/sticker-manager/releases/latest)，免费下载，不用注册账号。

## macOS

1. 打开[发布页](https://github.com/wjiajian/sticker-manager/releases/latest)，下载 `sticker-manager_<版本>_aarch64.dmg`。目前只发布 Apple 芯片版本，Intel Mac 暂时不能用。
2. 双击打开 DMG，把「表情管理器」拖进「应用程序」文件夹。
3. 按下一节的步骤放行，然后打开。

### 第一次打开时放行

:::caution[这是预期的情况]
安装包只做了临时签名，没有 Apple 开发者签名，也没有经过公证，所以第一次打开时 macOS 会把它拦下来。按下面的步骤放行一次就行。
:::

1. 在「应用程序」里双击「表情管理器」。系统会提示无法验证这个应用，点「完成」（有的系统版本是「好」）关掉提示，**不要**点「移到废纸篓」。
2. 打开「系统设置 › 隐私与安全性」，滚动到底部的「安全性」。
3. 那里会有一行提示，说「表情管理器」已被阻止打开，点旁边的「仍要打开」。没看到这一行的话，回到第 1 步再打开一次。
4. 按要求输入登录密码或者用触控 ID 确认，然后在弹出的对话框里再点一次「仍要打开」。

系统提示的具体措辞随 macOS 版本略有不同。如果提示的是「已损坏」，看[常见问题](./faq/#提示已损坏或者打不开怎么办)。

## Windows

发布页上有两种 Windows 版本，选一种下载。

### 安装版

1. 下载 `sticker-manager_<版本>_x64-setup.exe`。
2. 运行安装程序，按提示完成安装。

安装包没有代码签名，运行时系统可能弹出提示。出现「Windows 已保护你的电脑」时，点「更多信息」，再点「仍要运行」。

### 便携版

1. 下载 `sticker-manager_<版本>_windows-x64-portable.zip`。
2. 把整个 `sticker-manager` 文件夹解压到可写的位置。
3. 运行里面的 `sticker-manager.exe`。

便携版需要 WebView2 Runtime。程序旁的 `portable.flag` 用于启用便携模式，`data` 保存仓库，`webview` 保存缓存。

便携版不登记开机自启，也不使用安装式更新。升级时从菜单退出，再替换 EXE，保留 `portable.flag`、`data` 和 `webview`。搬到别的位置时，移动整个文件夹。

## 装好之后

看[快速上手](./quick-start/)，几分钟就能用起来。
