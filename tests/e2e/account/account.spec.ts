import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

import { resetAccount } from '../api';
import { PASSWORD, USERS } from '../fixtures';
import { AccountPage } from '../pages/AccountPage';
import { AppShell } from '../pages/AppShell';
import { OnboardingPage } from '../pages/OnboardingPage';
import { SignInPage } from '../pages/SignInPage';

const TEAM = 'Account E2E';
const AVATAR = path.join(__dirname, '..', 'assets', 'avatar.png');

async function signInAsDana(page: Page): Promise<AppShell> {
  const signInPage = new SignInPage(page);
  await signInPage.goto();
  await signInPage.signIn(USERS.dana.username, PASSWORD);
  const shell = new AppShell(page);
  await expect(shell.currentUser).toHaveText(USERS.dana.name);
  return shell;
}

test.describe('your account', () => {
  let shell: AppShell;
  let account: AccountPage;

  test.beforeEach(async ({ page }) => {
    await resetAccount(USERS.dana.username);
    shell = await signInAsDana(page);
    // Dana has no team after the reset: create the first one from onboarding.
    await new OnboardingPage(page).createTeamNamed(TEAM);
    await expect(shell.teamSelector).toContainText(TEAM);
    account = new AccountPage(page);
    await account.goto();
  });

  // Other specs expect dana under her seeded name.
  test.afterEach(async () => {
    await resetAccount(USERS.dana.username);
  });

  test('rename: the new display name survives a reload', async ({ page }) => {
    await account.rename('Dana E2E');
    await expect(page.getByText('Saved', { exact: true })).toBeVisible();
    await expect(shell.currentUser).toHaveText('Dana E2E');

    await page.reload();
    await expect(account.displayName).toHaveValue('Dana E2E');
    await expect(shell.currentUser).toHaveText('Dana E2E');
  });

  test('picture: an uploaded image shows in the top bar', async () => {
    await account.uploadPicture(AVATAR);
    await expect(shell.userMenuButton.locator('img')).toHaveAttribute('src', /^blob:/);
  });

  test('theme: choosing Dark sets the page theme', async ({ page }) => {
    await account.chooseTheme('Dark');
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    await expect(
      page
        .getByRole('region', { name: 'Preferences' })
        .getByRole('button', { name: 'Dark', exact: true }),
    ).toHaveAttribute('aria-pressed', 'true');
  });

  test('teams: leaving as the sole member confirms, then removes the team', async ({ page }) => {
    const row = page.getByTestId('team-row').filter({ hasText: TEAM });
    await expect(row).toBeVisible();
    await account.leave(TEAM);

    const dialog = page.getByRole('dialog', { name: `Leave and delete ${TEAM}?` });
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: 'Leave and delete team' }).click();

    await expect(dialog).toBeHidden();
    await expect(row).toHaveCount(0);
  });

  for (const theme of ['Light', 'Dark'] as const) {
    test(`mobile: no horizontal scroll at 390 px in the ${theme.toLowerCase()} theme`, async ({
      page,
    }) => {
      await account.chooseTheme(theme);
      await page.setViewportSize({ width: 390, height: 844 });
      await expect(page.locator('html')).toHaveAttribute('data-theme', theme.toLowerCase());
      await expect(page.getByTestId('team-row').filter({ hasText: TEAM })).toBeVisible();

      await expect
        .poll(() =>
          page.evaluate(
            () => document.documentElement.clientWidth - document.documentElement.scrollWidth,
          ),
        )
        .toBeGreaterThanOrEqual(0);
    });
  }
});
