import { expect, test } from '@playwright/test';

import { PASSWORD, USERS } from '../fixtures';
import { DexLoginPage, SignInPage } from '../pages/SignInPage';
import { AppShell } from '../pages/AppShell';

test.describe('Sign in', () => {
  test('redirects signed-out visitors to the sign-in page', async ({ page }) => {
    await page.goto('/');
    await expect(page).toHaveURL(/\/login$/);
    await expect(page.getByRole('heading', { name: 'Sign in to Retro' })).toBeVisible();
  });

  test('shows the design copy', async ({ page }) => {
    const signIn = new SignInPage(page);
    await signIn.goto();
    await expect(page.getByText('Run better retrospectives with your team.')).toBeVisible();
    await expect(signIn.remember).toBeVisible();
    await expect(signIn.sso).toBeVisible();
    await expect(
      page.getByText("Single sign-on takes you to your organization's sign-in page"),
    ).toBeVisible();
  });

  test('signs in with a username and password, then signs out', async ({ page }) => {
    const signIn = new SignInPage(page);
    await signIn.goto();
    await signIn.signIn(USERS.alice.username, PASSWORD);

    await expect(page).toHaveURL(AppShell.HOME);
    await expect(page.getByTestId('current-user')).toHaveText(USERS.alice.name);

    await new AppShell(page).logOut();
    await expect(page).toHaveURL(/\/login$/);
    await page.goto('/');
    await expect(page).toHaveURL(/\/login$/);
  });

  test('keeps the session across reloads with "Keep me signed in"', async ({ page, context }) => {
    const signIn = new SignInPage(page);
    await signIn.goto();
    await signIn.signIn(USERS.bob.email, PASSWORD, { remember: true });
    await expect(page.getByTestId('current-user')).toHaveText(USERS.bob.name);

    const refresh = (await context.cookies()).find((c) => c.name === 'retro.refresh');
    expect(refresh?.httpOnly).toBe(true);
    expect(refresh?.expires).toBeGreaterThan(Date.now() / 1000 + 24 * 3600);

    await page.reload();
    await expect(page.getByTestId('current-user')).toHaveText(USERS.bob.name);
  });

  test('without "Keep me signed in" the cookie ends with the browser session', async ({
    page,
    context,
  }) => {
    const signIn = new SignInPage(page);
    await signIn.goto();
    await signIn.signIn(USERS.alice.username, PASSWORD);
    await expect(page.getByTestId('current-user')).toBeVisible();
    const refresh = (await context.cookies()).find((c) => c.name === 'retro.refresh');
    expect(refresh?.expires).toBe(-1);
  });

  test('rejects a wrong password', async ({ page }) => {
    const signIn = new SignInPage(page);
    await signIn.goto();
    await signIn.signIn(USERS.alice.username, 'not-the-password');
    await expect(signIn.error).toHaveText('Incorrect username or password.');
    await expect(signIn.password).toHaveValue('');
    await expect(page).toHaveURL(/\/login/);
  });

  test('reveals the password on demand', async ({ page }) => {
    const signIn = new SignInPage(page);
    await signIn.goto();
    await signIn.password.fill('secret');
    await expect(signIn.password).toHaveAttribute('type', 'password');
    await signIn.togglePassword.click();
    await expect(signIn.password).toHaveAttribute('type', 'text');
  });

  test('returns to the requested page after signing in', async ({ page }) => {
    await page.goto('/');
    const signIn = new SignInPage(page);
    await signIn.goto('?redirect=%2F');
    await signIn.signIn(USERS.alice.username, PASSWORD);
    await expect(page).toHaveURL(AppShell.HOME);
  });

  test('signs in with single sign-on through dex', async ({ page }) => {
    const signIn = new SignInPage(page);
    await signIn.goto();
    await signIn.sso.click();
    await new DexLoginPage(page).signIn(USERS.bob.email, PASSWORD);

    await expect(page).toHaveURL(AppShell.HOME);
    await expect(page.getByTestId('current-user')).toHaveText(USERS.bob.name);
    // No token ever lands in the URL.
    expect(page.url()).not.toContain('token');
  });

  test('uses the dark theme when the system prefers it', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    const signIn = new SignInPage(page);
    await signIn.goto();
    const background = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(background).toBe('rgb(30, 25, 48)');
  });
});
