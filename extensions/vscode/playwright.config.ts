import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './test', testMatch: '**/*.spec.ts', fullyParallel: true,
  use: { baseURL: 'http://127.0.0.1:4178', viewport: { width: 1100, height: 900 }, screenshot: 'only-on-failure' },
  webServer: { command: 'npx tsx test/server.ts', port: 4178, reuseExistingServer: false },
});
