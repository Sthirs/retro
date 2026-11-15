import type { Page } from '@playwright/test';

/**
 * Collects the page's Content Security Policy violations, across navigations
 * (frontend/nginx.conf sets the policy). Call before the first `goto`.
 * Playwright's browser runs no extensions, so anything reported is the app's.
 */
export async function watchCsp(page: Page): Promise<string[]> {
  const violations: string[] = [];
  await page.exposeFunction('__reportCspViolation', (violation: string) => {
    violations.push(violation);
  });
  await page.addInitScript(() => {
    document.addEventListener('securitypolicyviolation', (e) => {
      const report = (window as unknown as { __reportCspViolation(v: string): void })
        .__reportCspViolation;
      report(`${e.violatedDirective} blocked ${e.blockedURI} (${e.sourceFile}:${e.lineNumber})`);
    });
  });
  return violations;
}
