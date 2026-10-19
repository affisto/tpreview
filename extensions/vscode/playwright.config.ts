import { defineConfig } from '@playwright/test';
const port = Number(process.env.TPREVIEW_TEST_PORT ?? 4178);
export default defineConfig({
  testDir: './test', testMatch: '**/*.spec.ts', fullyParallel: true,
  use: { baseURL: `http://127.0.0.1:${port}`, viewport: { width: 1100, height: 900 }, screenshot: 'only-on-failure' },
  webServer: { command: 'npx tsx test/server.ts', port, reuseExistingServer: false },
});
