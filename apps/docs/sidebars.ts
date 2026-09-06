import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    {
      type: 'doc',
      id: 'index',
      label: 'Home',
    },
    {
      type: 'category',
      label: 'Guide',
      items: [
        'guide/getting-started',
        'guide/installation',
        'guide/sync',
        'guide/import',
        'guide/basic-usage',
        'guide/tui',
        'guide/agents',
        'guide/secrets',
      ],
    },
    'configuration',
    {
      type: 'category',
      label: 'Reference',
      items: [
        'reference/index',
        'reference/login',
        'reference/logout',
        'reference/setup',
        'reference/init',
        'reference/sync',
        'reference/status',
        'reference/list',
        'reference/doctor',
        'reference/targets',
        'reference/use',
        'reference/create',
        'reference/capture',
        'reference/delete',
        'reference/unuse',
        'reference/migrate',
        'reference/update',
        'reference/tui',
      ],
    },
    'self-hosting',
    'faq',
    'uninstall',
  ],
};

export default sidebars;
