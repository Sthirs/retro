/*
 * Config for the README screenshot generator (scripts/screenshots/).
 *
 * Deliberately SEPARATE from playwright.config.ts: the end-to-end suite runs
 * everything under tests/e2e/, and these captures are doc-asset generation, not
 * assertions. A flaky shot must never fail the test suite, and CI has no reason
 * to render PNGs into a throwaway runner.
 *
 * Like the e2e config, it expects the app to be deployed already; pass the host
 * to `npm run screenshots`.
 */
import { defineConfig, devices } from '@playwright/test';

const baseURL = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';

export default defineConfig({
  testDir: './scripts/screenshots',
  fullyParallel: false,
  // The captures share one dataset and one page; they must not interleave.
  workers: 1,
  retries: 0,
  timeout: 180_000,
  reporter: [['list']],
  use: {
    baseURL,
    // The dev cluster serves a local mkcert certificate.
    ignoreHTTPSErrors: true,
    // Pin locale, time zone and theme so the committed assets do not depend on
    // the machine that generated them.
    locale: 'en-US',
    timezoneId: 'Europe/Rome',
    colorScheme: 'dark',
    trace: 'retain-on-failure',
    ...devices['Desktop Chrome'],
    // 16:10 desktop. deviceScaleFactor 1 keeps every PNG exactly 1440x900 and
    // under the 500 KB check-added-large-files limit.
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 1,
  },
  projects: [{ name: 'desktop' }],
});
