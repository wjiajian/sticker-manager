# 官网

表情管理器的软件官网和使用文档，用 [Astro](https://astro.build/) 和 [Starlight](https://starlight.astro.build/) 构建，部署在 GitHub Pages：<https://wjiajian.github.io/sticker-manager/>。

官网的代码、依赖和构建配置都放在这个目录里，和应用的 `frontend/` 互不依赖。

## 本地开发

下面的命令在仓库根目录运行：

```bash
pnpm --dir website install
pnpm --dir website dev
pnpm --dir website build
```

`pnpm --dir website preview` 可以预览构建结果。地址带子路径 `/sticker-manager/`，和线上一致。

## 目录

| 路径 | 内容 |
|---|---|
| `src/pages/` | 首页和 404 |
| `src/components/` | 首页区块、导航和页脚 |
| `src/content/docs/docs/` | 使用文档，路由是 `/docs/` |
| `src/data/` | 链接、表情列表和发布信息 |
| `src/styles/` | 配色变量、首页样式、文档样式覆盖 |
| `src/assets/stickers/` | 页面里用到的表情，构建时转成 WebP |
| `public/` | 图标、分享图和动图 |

`public/og.png` 和 `public/media/cooking.gif` 是生成好的文件，改动时直接替换。

## 版本号和日期

首屏的版本号和发布日期在构建时从 GitHub API 读取，见 `src/data/release.ts`。读不到时退回 `src-tauri/tauri.conf.json` 的版本号，不显示日期。

## 部署

`.github/workflows/deploy-website.yml` 在 `website/` 有改动推到 `main` 时运行。两个发版 workflow 发布 Release 后也会触发它，让首屏显示新版本。

仓库的 Settings › Pages 里，Source 要选 GitHub Actions。

## 表情素材

页面里的表情不在本项目的 MIT 许可范围内，来源和限制见 [CREDITS.md](CREDITS.md)。新增表情时同步更新那份列表，并写明来源。
