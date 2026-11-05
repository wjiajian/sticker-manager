import { version as localVersion } from '../../../src-tauri/tauri.conf.json';
import { REPO } from './links';

export interface Release {
  version: string;
  /** 例如「11 月 5 日发布」。没有拿到发布时间时为 null。 */
  released: string | null;
}

/** 构建时读取最新 Release。读不到时退回仓库里的版本号，不显示日期。 */
async function loadRelease(): Promise<Release> {
  try {
    const headers: Record<string, string> = { Accept: 'application/vnd.github+json' };
    if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;

    const response = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers });
    if (!response.ok) throw new Error(`GitHub 返回 ${response.status}`);

    const { tag_name: tag, published_at: publishedAt } = (await response.json()) as {
      tag_name: string;
      published_at: string;
    };
    return { version: tag.replace(/^v/, ''), released: formatDate(new Date(publishedAt)) };
  } catch (error) {
    console.warn(`读取最新 Release 失败，改用仓库里的版本号：${String(error)}`);
    return { version: localVersion, released: null };
  }
}

function formatDate(date: Date): string {
  const parts = Object.fromEntries(
    new Intl.DateTimeFormat('en-US', {
      timeZone: 'Asia/Shanghai',
      year: 'numeric',
      month: 'numeric',
      day: 'numeric',
    })
      .formatToParts(date)
      .map(({ type, value }) => [type, value]),
  );
  const year = Number(parts.year) === new Date().getFullYear() ? '' : `${parts.year} 年 `;
  return `${year}${parts.month} 月 ${parts.day} 日发布`;
}

export const release = await loadRelease();
