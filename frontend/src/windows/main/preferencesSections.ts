import { ArchiveIcon, ArrowCircleUpIcon, CloudIcon, ExportIcon, KeyboardIcon, PaletteIcon, PowerIcon, SmileyIcon } from "@phosphor-icons/react";

export const SECTIONS = [
  { id: "appearance", label: "外观", icon: PaletteIcon, items: [
    { id: "appearance-mode", label: "界面外观", keywords: ["主题 深色 浅色 暗色 夜间 跟随系统 dark light theme"] },
  ] },
  { id: "startup", label: "启动", icon: PowerIcon, items: [
    { id: "autostart", label: "开机时自动启动", keywords: ["登录 autostart login"] },
  ] },
  { id: "ball", label: "悬浮球", icon: SmileyIcon, items: [
    { id: "ball-visible", label: "显示悬浮球", keywords: ["隐藏"] },
    { id: "ball-autohide", label: "静止后收到屏幕边缘", keywords: ["自动收起 贴边 3 秒 10 秒 不收起"] },
    { id: "ball-size", label: "悬浮球大小", keywords: ["尺寸 像素 px 恢复默认"] },
  ] },
  { id: "panel", label: "表情面板", icon: KeyboardIcon, items: [
    { id: "panel-shortcut", label: "呼出快捷键", keywords: ["热键 组合键 停用 恢复默认 shortcut hotkey"] },
  ] },
  { id: "sync", label: "同步", icon: CloudIcon, items: [
    { id: "sync", label: "存储后端", keywords: ["云 文件夹 iCloud Dropbox WebDAV R2 S3 Cloudflare 设备名 流量 暂停 密码 凭据 管理配置"] },
  ] },
  { id: "library", label: "仓库", icon: ArchiveIcon, items: [
    { id: "library-path", label: "仓库路径", keywords: ["位置 目录 文件夹 打开 访达 资源管理器 复制"] },
    { id: "library-move", label: "移动位置", keywords: ["迁移 恢复默认 磁盘 文件夹"] },
  ] },
  { id: "export", label: "导出", icon: ExportIcon, items: [
    { id: "export", label: "导出仓库", keywords: ["备份 zip 压缩包 图片 文件夹"] },
  ] },
  { id: "update", label: "更新", icon: ArrowCircleUpIcon, items: [
    { id: "update-check", label: "检查更新", keywords: ["版本 升级 下载"] },
    { id: "update-auto", label: "自动检查更新", keywords: ["自动更新"] },
  ] },
] as const;

export type SectionId = typeof SECTIONS[number]["id"];
export type ItemId = typeof SECTIONS[number]["items"][number]["id"];
const normalize = (value: string) => value.normalize("NFKC").toLowerCase().trim();

export function matchPreferences(query: string) {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  const sections = new Set<SectionId>();
  const items = new Set<ItemId>();
  for (const section of SECTIONS) {
    for (const item of section.items) {
      const text = normalize([section.label, item.label, ...item.keywords].join(" "));
      if (words.every(word => text.includes(word))) {
        sections.add(section.id);
        items.add(item.id);
      }
    }
  }
  return { sections, items, empty: items.size === 0 };
}
