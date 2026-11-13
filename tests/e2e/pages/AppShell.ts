import { expect, type Locator, type Page } from '@playwright/test';

/** Signed-in chrome: top bar (team selector, user menu) and the team's left menu. */
export class AppShell {
  readonly currentUser: Locator;
  readonly teamSelector: Locator;
  readonly teamMenu: Locator;
  readonly userMenuButton: Locator;
  readonly userMenu: Locator;

  constructor(private readonly page: Page) {
    this.currentUser = page.getByTestId('current-user');
    this.teamSelector = page.getByTestId('team-selector');
    this.teamMenu = page.getByRole('menu', { name: 'Teams' });
    this.userMenuButton = page.getByRole('button', { name: 'User menu' });
    this.userMenu = page.locator('#user-menu');
  }

  /** Where signed-in users land: a team's meetings, or onboarding. */
  static readonly HOME = /\/(teams\/[^/]+\/meetings)?$/;

  async logOut(): Promise<void> {
    await this.userMenuButton.click();
    await this.userMenu.getByRole('button', { name: 'Log out' }).click();
    await expect(this.page).toHaveURL(/\/login$/);
  }

  async addTeam(name: string): Promise<void> {
    await this.teamSelector.click();
    await this.teamMenu.getByRole('menuitem', { name: 'Add team' }).click();
    const dialog = this.page.getByRole('dialog', { name: 'Create a team' });
    await dialog.getByLabel('Team name').fill(name);
    await dialog.getByRole('button', { name: 'Create team' }).click();
    await expect(dialog).toBeHidden();
    await expect(this.teamSelector).toContainText(name);
  }

  async openSection(name: 'Meetings' | 'Tasks' | 'Team settings'): Promise<void> {
    await this.page.getByRole('navigation', { name: 'Team' }).getByRole('link', { name }).click();
  }
}
