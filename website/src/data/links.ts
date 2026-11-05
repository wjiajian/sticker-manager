export const REPO = 'wjiajian/sticker-manager';
export const REPO_URL = `https://github.com/${REPO}`;
export const RELEASES_URL = `${REPO_URL}/releases/latest`;
export const ISSUES_URL = `${REPO_URL}/issues`;
export const CHANGELOG_URL = `${REPO_URL}/blob/main/CHANGELOG.md`;
export const LICENSE_URL = `${REPO_URL}/blob/main/LICENSE`;
export const STICKER_PACK_URL = 'https://github.com/anka-afk/deepseek-fanart-meme-pack';

/** 站内路径，带上 GitHub Pages 的子路径。 */
export function sitePath(path = ''): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, '');
  return `${base}/${path.replace(/^\//, '')}`;
}
