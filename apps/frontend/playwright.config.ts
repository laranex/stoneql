import { defineConfig } from '@playwright/test';
// STONEQL_BROWSER_PORT moves the owned dev server off 5173 when another local app holds it.
const port = Number(process.env.STONEQL_BROWSER_PORT ?? 5173);
export default defineConfig({
  testDir: './tests/browser',
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    viewport: { width: 1440, height: 1000 },
    // Keep Chromium's sandbox enabled; a restricted runner must fail rather than disable it.
    launchOptions: { chromiumSandbox: true },
  },
  webServer: {
    command: `npm run dev -- --port ${port} --strictPort`,
    url: `http://127.0.0.1:${port}`,
    reuseExistingServer: !process.env.CI,
  },
});
