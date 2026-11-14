import { expect, type Locator, type Page } from '@playwright/test';

import { AppShell } from './AppShell';

/** `/account`, reached through the user menu. */
export class AccountPage {
  readonly heading: Locator;
  readonly displayName: Locator;
  readonly save: Locator;
  readonly uploadInput: Locator;

  constructor(private readonly page: Page) {
    this.heading = page.getByRole('heading', { level: 1, name: 'Your account' });
    this.displayName = page.getByLabel('Display name');
    this.save = page.getByRole('button', { name: 'Save changes' });
    this.uploadInput = page.locator('input[type="file"]');
  }

  async goto(): Promise<void> {
    const shell = new AppShell(this.page);
    await shell.userMenuButton.click();
    await shell.userMenu.getByRole('link', { name: 'Your account' }).click();
    await expect(this.page).toHaveURL(/\/account$/);
    await expect(this.heading).toBeVisible();
  }

  async rename(name: string): Promise<void> {
    await this.displayName.fill(name);
    await this.save.click();
  }

  async uploadPicture(path: string): Promise<void> {
    await this.uploadInput.setInputFiles(path);
  }

  async chooseTheme(theme: 'Light' | 'Dark' | 'System'): Promise<void> {
    await this.page
      .getByRole('region', { name: 'Preferences' })
      .getByRole('button', { name: theme, exact: true })
      .click();
  }

  /** Opens the leave confirmation of `team`; the caller answers the dialog. */
  async leave(team: string): Promise<void> {
    await this.page.getByRole('button', { name: `Leave ${team}` }).click();
  }
}
