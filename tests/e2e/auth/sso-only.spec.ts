import { expect, test } from '@playwright/test';

import { PASSWORD, USERS } from '../fixtures';
import { DexLoginPage, SignInPage } from '../pages/SignInPage';

/** carol exists only in dex: single sign-on works, password sign-in never does. */
test.describe('SSO-only account', () => {
  test('signs in through dex but not with the password form', async ({ page }) => {
    const signIn = new SignInPage(page);
    const carol = USERS.carol;

    // Password sign-in fails before her first SSO sign-in…
    await signIn.goto();
    await signIn.signIn(carol.email, PASSWORD);
    await expect(signIn.error).toHaveText('Incorrect username or password.');

    // …single sign-on works (and creates her account on first use)…
    await signIn.sso.click();
    await new DexLoginPage(page).signIn(carol.email, PASSWORD);
    await expect(page).toHaveURL(/\/$/);
    await expect(page.getByTestId('current-user')).toHaveText(carol.name);

    // …and password sign-in still fails once the account exists: it has no password.
    await page.getByRole('button', { name: 'Sign out' }).click();
    await expect(page).toHaveURL(/\/login$/);
    await signIn.signIn(carol.email, PASSWORD);
    await expect(signIn.error).toHaveText('Incorrect username or password.');
    await expect(page).toHaveURL(/\/login/);
  });
});
