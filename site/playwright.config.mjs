import { defineConfig, devices } from '@playwright/test';

// The built site, served as plain files (scripts/serve.mjs). Build first:
// `npm run build`, as `npm run test:e2e` does through its pretest hook.
const port = 4173;

export default defineConfig({
	testDir: 'e2e',
	testMatch: '*.spec.mjs',
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	reporter: process.env.CI ? 'line' : 'list',
	use: { baseURL: `http://localhost:${port}` },
	webServer: {
		command: 'node scripts/serve.mjs',
		url: `http://localhost:${port}/`,
		env: { PORT: String(port) },
		reuseExistingServer: !process.env.CI
	},
	projects: [
		{
			name: 'desktop',
			use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 } }
		},
		{
			// The generated frame test is the 1440 design's; the phone runs the rest.
			name: 'phone',
			testIgnore: 'frame.spec.mjs',
			use: { ...devices['Pixel 7'], viewport: { width: 390, height: 844 } }
		}
	]
});
