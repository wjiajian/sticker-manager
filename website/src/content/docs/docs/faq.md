---
title: 常见问题
description: 打不开、同步提示、备份，以及官网表情的来源。
---

## 提示「已损坏」或者打不开怎么办？

1. 按[第一次打开时放行](../#第一次打开时放行)的步骤，在「系统设置 › 隐私与安全性」里点「仍要打开」。
2. 还是不行，可以在终端里去掉系统给下载文件加的隔离标记，然后重新打开：

   ```bash
   xattr -cr /Applications/表情管理器.app
   ```

## 设置同步时提示「这个路径非空且没有 library.json」？

同步只能建在空的位置，或者连到一个已有的同步仓库。请换一个空文件夹或空路径。想把现有的图片收进来，先在应用里导入，或者等同步建好后再把图片放进同步目录。

## 怎么备份？

看[数据和备份](../data/#怎么备份)。

## Intel Mac 能用吗？

暂时不能，目前只发布 Apple 芯片版本。

## 能在哪些聊天软件里用？

能粘贴图片的地方都能用，比如微信、QQ 和飞书。粘贴后显示成图片还是文件，由聊天软件决定。

## 官网上的表情是哪来的？

官网页面里的表情来自 [deepseek-fanart-meme-pack](https://github.com/anka-afk/deepseek-fanart-meme-pack)，原作者未知，与 DeepSeek 官方无关，也不在本项目的 MIT 许可范围内。权利人可以通过 [GitHub Issue](https://github.com/wjiajian/sticker-manager/issues) 要求移除。

## 怎样反馈问题？

在 [GitHub Issue](https://github.com/wjiajian/sticker-manager/issues) 里提，写上系统版本和应用版本。
