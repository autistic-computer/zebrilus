// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
	site: 'https://zzmux.sh',
	base: '/',
	redirects: { '/docs': '/docs/getting-started' },
	integrations: [
		starlight({
			title: 'zz',
			description: 'Terminal and browser. One mux.',
			head: [
				{ tag: 'meta', attrs: { property: 'og:image', content: 'https://zzmux.sh/og.png' } },
				{ tag: 'meta', attrs: { property: 'og:image:width', content: '1200' } },
				{ tag: 'meta', attrs: { property: 'og:image:height', content: '630' } },
				{ tag: 'meta', attrs: { name: 'twitter:image', content: 'https://zzmux.sh/og.png' } },
			],
			logo: {
				light: './src/assets/zz-on-light.svg',
				dark: './src/assets/zz.svg',
				replacesTitle: false,
			},
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/autistic-computer/zz' }],
			customCss: ['./src/styles/custom.css'],
			expressiveCode: {
				styleOverrides: {
					borderRadius: '14px',
					borderColor: 'rgba(128, 128, 128, 0.26)',
				},
			},
			sidebar: [
				{ label: 'Getting started', slug: 'docs/getting-started' },
				{ label: 'tmux compatibility', slug: 'docs/tmux' },
				{ label: 'Browser panes', slug: 'docs/browser' },
				{
					label: 'Agent panes',
					slug: 'docs/agents',
					badge: { text: 'Upcoming', variant: 'caution' },
				},
				{ label: 'CLI', slug: 'docs/cli' },
				{ label: 'Configuration', slug: 'docs/configuration' },
			],
		}),
	],
});
