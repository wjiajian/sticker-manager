import type { ImageMetadata } from 'astro';
import { getImage } from 'astro:assets';

const files = import.meta.glob<{ default: ImageMetadata }>('../assets/stickers/*.png', {
  eager: true,
});

export interface Sticker {
  key: string;
  name: string;
  /** 演示面板搜索用的文字：名称、配文和分类。 */
  keywords: string;
}

export const stickers: Sticker[] = [
  { key: 'zuiqiang', name: '最强', keywords: '最强 开心 厉害 夸奖' },
  { key: 'taibangle', name: '小肥鱼太棒了', keywords: '小肥鱼太棒了 开心 夸奖' },
  { key: 'zhouwu', name: '周五周五', keywords: '周五 开心 敲锣打鼓 下班' },
  { key: 'ciallo', name: 'Ciallo', keywords: 'ciallo 开心 你好 打招呼' },
  { key: 'zuohaole', name: '我坐好了', keywords: '我坐好了 开心 回复 收到' },
  { key: 'bulai', name: '不赖', keywords: '不赖 开心 夸奖 困惑' },
  { key: 'dadeyaolaila', name: '大的要来啦', keywords: '大的要来啦 开心 预告' },
  { key: 'xiaban', name: '下班', keywords: '下班 开心 打工' },
  { key: 'moyu', name: '摸鱼', keywords: '摸鱼 下班 打工' },
  { key: 'touxian', name: '我喜欢偷懒', keywords: '我喜欢偷懒 摸鱼 下班 打工' },
  { key: 'chibaobao', name: '吃饱饱', keywords: '吃饱饱 下班 睡觉 晚安' },
  { key: 'youele', name: '又饿了', keywords: '又饿了 吃饭 下班 美食' },
  { key: 'nandaoshuo', name: '难道说', keywords: '难道说 困惑 震惊' },
  { key: 'huozhe', name: '动一下还活着吗', keywords: '动一下还活着吗 困惑 关心' },
  { key: 'ji', name: '寄', keywords: '寄 悲伤 困惑 完了' },
  { key: 'sidiaole', name: '死掉了', keywords: '死掉了 悲伤 累' },
  { key: 'bukeyi', name: '不可以', keywords: '不可以 禁止 拒绝 生气' },
  { key: 'nicaisha', name: '你才傻', keywords: '你才傻 生气 回怼' },
];

/** 按需要的显示宽度输出 WebP 图片地址，宽度按两倍屏计算。 */
export async function stickerSrc(key: string, displayWidth: number): Promise<string> {
  const file = files[`../assets/stickers/${key}.png`];
  if (!file) throw new Error(`缺少表情图片：${key}`);
  const width = Math.min(displayWidth * 2, file.default.width);
  const image = await getImage({ src: file.default, width, format: 'webp' });
  return image.src;
}

export function matches(sticker: Sticker, query: string): boolean {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return terms.every((term) => sticker.keywords.includes(term));
}
