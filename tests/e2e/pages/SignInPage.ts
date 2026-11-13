import { expect, type Locator, type Page } from '@playwright/test';

export class SignInPage {
  readonly heading: Locator;
  readonly login: Locator;
  readonly password: Locator;
  readonly togglePassword: Locator;
  readonly remember: Locator;
  readonly submit: Locator;
  readonly sso: Locator;
  readonly error: Locator;

  constructor(private readonly page: Page) {
    this.heading = page.getByRole('heading', { name: 'Sign in to Retro' });
    this.login = page.getByLabel('Username or email');
    this.password = page.getByLabel('Password', { exact: true });
    this.togglePassword = page.getByRole('button', { name: /show password|hide password/i });
    this.remember = page.getByLabel('Keep me signed in');
    this.submit = page.getByRole('button', { name: 'Sign in', exact: true });
    this.sso = page.getByRole('link', { name: /single sign-on/i });
    this.error = page.getByRole('alert');
  }

  async goto(query = ''): Promise<void> {
    await this.page.goto(`/login${query}`);
    await expect(this.heading).toBeVisible();
    await expect(this.submit).toBeVisible();
  }

  async signIn(login: string, password: string, { remember = false } = {}): Promise<void> {
    await this.login.fill(login);
    await this.password.fill(password);
    if (remember) await this.remember.check();
    await this.submit.click();
  }
}

/** dex's own login page. */
export class DexLoginPage {
  constructor(private readonly page: Page) {}

  async signIn(email: string, password: string): Promise<void> {
    await this.page.waitForURL(/\/dex\//);
    // With one connector dex may show a connector chooser first.
    const local = this.page.getByRole('link', { name: /log in with email/i });
    if (await local.isVisible().catch(() => false)) await local.click();
    await this.page.locator('input[name="login"]').fill(email);
    await this.page.locator('input[name="password"]').fill(password);
    await this.page.locator('button[type="submit"]').click();
    const grant = this.page.getByRole('button', { name: /grant access/i });
    if (await grant.isVisible({ timeout: 1000 }).catch(() => false)) await grant.click();
  }
}
