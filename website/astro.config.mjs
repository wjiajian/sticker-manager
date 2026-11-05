import starlight from '@astrojs/starlight';
import { defineConfig } from 'astro/config';

const description =
  '在任何应用里按快捷键呼出表情面板，输入文字搜索，单击复制。发出去的是原图，免费开源，不用注册账号。';

export default defineConfig({
  site: 'https://wjiajian.github.io',
  base: '/sticker-manager',
  trailingSlash: 'always',
  integrations: [
    starlight({
      title: '表情管理器',
      description,
      favicon: '/favicon.svg',
      head: [
        { tag: 'link', attrs: { rel: 'apple-touch-icon', href: '/sticker-manager/apple-touch-icon.png' } },
        { tag: 'meta', attrs: { property: 'og:image', content: 'https://wjiajian.github.io/sticker-manager/og.png' } },
        { tag: 'meta', attrs: { name: 'twitter:card', content: 'summary_large_image' } },
      ],
      logo: { src: './src/assets/icon.png', alt: '' },
      locales: { root: { label: '简体中文', lang: 'zh-CN' } },
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/wjiajian/sticker-manager' },
      ],
      // 404 由 src/pages/404.astro 提供，和首页用同一套外观。
      disable404Route: true,
      customCss: ['@fontsource-variable/manrope/wght.css', './src/styles/starlight.css'],
      components: {
        // 全站跟随系统的浅色和深色，不单独提供切换。
        ThemeSelect: './src/components/StarlightThemeSelect.astro',
      },
      sidebar: [
        { label: '开始使用', items: ['docs', 'docs/quick-start'] },
        {
          label: '功能',
          items: [
            'docs/import',
            'docs/organize',
            'docs/panel',
            'docs/floating-ball',
            'docs/preferences',
            'docs/sync',
            'docs/updates',
          ],
        },
        { label: '数据和帮助', items: ['docs/data', 'docs/faq'] },
      ],
    }),
  ],
});
