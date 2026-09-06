import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'skl',
  tagline: 'Sync your agent skills across machines and projects.',
  favicon: 'img/favicon.ico',

  future: {
    v4: true,
  },

  url: 'https://docs.tryskl.fyi',
  baseUrl: '/',

  organizationName: 'x0ba',
  projectName: 'skl',

  onBrokenLinks: 'throw',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/x0ba/skl/tree/main/docs/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    colorMode: {
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'skl',
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docsSidebar',
          position: 'left',
          label: 'Docs',
        },
        {
          href: 'https://www.tryskl.fyi/skills',
          label: 'Dashboard',
          position: 'left',
        },
        {
          href: 'https://github.com/x0ba/skl',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {label: 'Home', to: '/'},
            {label: 'Getting started', to: '/guide/getting-started'},
            {label: 'Reference', to: '/reference'},
          ],
        },
        {
          title: 'GitHub',
          items: [
            {label: 'x0ba/skl', href: 'https://github.com/x0ba/skl'},
            {label: 'Issues', href: 'https://github.com/x0ba/skl/issues'},
          ],
        },
        {
          title: 'Dashboard',
          items: [
            {label: 'Skills', href: 'https://www.tryskl.fyi/skills'},
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} skl.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
