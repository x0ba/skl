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
      label: 'Tutorial',
      items: ['tutorial/getting-started'],
    },
    {
      type: 'category',
      label: 'How-to guides',
      items: [
        'how-to/install',
        'how-to/import',
        'how-to/use',
        'how-to/cursor-cloud',
        'how-to/extras',
        'how-to/sync',
        'how-to/migrate',
        'how-to/update',
        'how-to/uninstall',
        'how-to/self-host',
      ],
    },
    {
      type: 'category',
      label: 'Reference',
      items: [
        'reference/index',
        {
          type: 'category',
          label: 'Commands',
          items: [
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
        'reference/configuration',
        'reference/skills-toml',
        'reference/secret-scanner',
        'reference/installer',
      ],
    },
    {
      type: 'category',
      label: 'Explanation',
      items: ['explanation/storage', 'explanation/copies', 'explanation/sync'],
    },
  ],
};

export default sidebars;
