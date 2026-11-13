// No webServer: the app must already be deployed (scripts/dev.sh) and
// reachable at PLAYWRIGHT_BASE_URL (scripts/test-playwright.sh sets it).
import { defineConfig, devices } from '@playwright/test';

const baseURL = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: false,
  workers: 2,
  timeout: 60_000,
  retries: process.env.CI ? 0 : 1,
  forbidOnly: !!process.env.CI,
  reporter: [['html', { open: 'never' }], ['list']],
  use: {
    baseURL,
    // The dev cluster serves a local mkcert certificate; trust is not under test.
    ignoreHTTPSErrors: true,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
