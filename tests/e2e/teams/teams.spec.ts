import { expect, test, type Browser, type Page } from '@playwright/test';

import { resetAccount } from '../api';
import { PASSWORD, USERS } from '../fixtures';
import { AppShell } from '../pages/AppShell';
import { InvitationPage } from '../pages/InvitationPage';
import { OnboardingPage } from '../pages/OnboardingPage';
import { SignInPage } from '../pages/SignInPage';
import { TeamSettingsPage } from '../pages/TeamSettingsPage';

// These tests change alice's and bob's teams, so they run one at a time.
test.describe.configure({ mode: 'serial' });

async function signIn(page: Page, user: keyof typeof USERS): Promise<void> {
  const signInPage = new SignInPage(page);
  await signInPage.goto();
  await signInPage.signIn(USERS[user].username, PASSWORD);
  await expect(new AppShell(page).currentUser).toHaveText(USERS[user].name);
}

/** A second, independent browser session (another person). */
async function otherUser(browser: Browser, user: keyof typeof USERS): Promise<Page> {
  const context = await browser.newContext();
  const page = await context.newPage();
  await signIn(page, user);
  return page;
}

test.describe('teams', () => {
  test.beforeEach(async () => {
    await resetAccount(USERS.alice.username);
    await resetAccount(USERS.bob.username);
  });

  test('onboarding: a user without teams creates one, then another', async ({ page }) => {
    await signIn(page, 'alice');
    const onboarding = new OnboardingPage(page);
    await expect(onboarding.heading).toHaveText('Welcome to Retro, Alice');
    await expect(
      page.getByText("You're not in a team yet. Create one, or accept an invitation"),
    ).toBeVisible();

    await onboarding.createTeamNamed('E2E Platform');
    const shell = new AppShell(page);
    await expect(shell.teamSelector).toContainText('E2E Platform');
    await expect(page.getByRole('heading', { level: 1, name: 'Meetings' })).toBeVisible();

    await shell.addTeam('E2E Mobile');
    await shell.teamSelector.click();
    await expect(shell.teamMenu.getByRole('menuitem')).toHaveText([
      /E2E Mobile/,
      /E2E Platform/,
      /Add team/,
    ]);
    await shell.teamMenu.getByRole('menuitem', { name: /E2E Platform/ }).click();
    await expect(shell.teamSelector).toContainText('E2E Platform');

    // The last used team opens next time.
    await page.goto('/');
    await expect(shell.teamSelector).toContainText('E2E Platform');
  });

  test('invitation: alice invites bob by email, bob accepts', async ({ page, browser }) => {
    await signIn(page, 'alice');
    const teamId = await new OnboardingPage(page).createTeamNamed('E2E Invite');
    await new AppShell(page).openSection('Team settings');
    const settings = new TeamSettingsPage(page);
    await expect(settings.heading).toBeVisible();
    const link = await settings.invite(USERS.bob.email);
    expect(link).toMatch(/\/invite\/[\w-]+$/);
    await expect(settings.pendingInvitations.filter({ hasText: USERS.bob.email })).toContainText(
      'Invited by you',
    );

    // Bob sees it on his onboarding screen and in the user menu…
    const bob = await otherUser(browser, 'bob');
    const onboarding = new OnboardingPage(bob);
    await expect(onboarding.invitation('E2E Invite')).toContainText(
      `Invited by ${USERS.alice.name}`,
    );
    await expect(bob.getByRole('button', { name: 'Pending invitations, 1 new' })).toBeVisible();

    // …and accepts through the personal link.
    await bob.goto(new URL(link).pathname);
    const invitation = new InvitationPage(bob);
    await expect(invitation.heading).toHaveText('Join E2E Invite');
    await expect(bob.getByText(`${USERS.alice.name} invited you to join their team`)).toBeVisible();
    await invitation.accept.click();
    await expect(bob).toHaveURL(new RegExp(`/teams/${teamId}/meetings$`));
    await expect(new AppShell(bob).teamSelector).toContainText('E2E Invite');

    await page.reload();
    await expect(settings.member(USERS.bob.name)).toContainText('Member');
    await expect(settings.pendingInvitations).toHaveCount(0);
    await bob.context().close();
  });

  test('settings: rename, join link, roles, removal and deletion', async ({ page, browser }) => {
    await signIn(page, 'alice');
    const teamId = await new OnboardingPage(page).createTeamNamed('E2E Settings');
    const settings = new TeamSettingsPage(page);
    await settings.goto(teamId);
    await expect(settings.subtitle).toHaveText("E2E Settings · you're an admin");

    // Rename.
    await settings.teamName.fill('E2E Renamed');
    await settings.save.click();
    await expect(settings.subtitle).toHaveText("E2E Renamed · you're an admin");
    await expect(new AppShell(page).teamSelector).toContainText('E2E Renamed');

    // Bob joins through the join link.
    await expect(settings.joinSwitch).toHaveAttribute('aria-checked', 'true');
    const joinLink = await settings.joinLink.inputValue();
    const bob = await otherUser(browser, 'bob');
    await bob.goto(new URL(joinLink).pathname);
    await expect(bob.getByRole('heading', { level: 1 })).toHaveText("You've joined E2E Renamed");
    await bob.getByRole('link', { name: 'Go to meetings' }).click();
    await expect(bob).toHaveURL(new RegExp(`/teams/${teamId}/meetings$`));

    // Promote, demote and remove bob.
    await page.reload();
    const bobRow = settings.member(USERS.bob.name);
    await expect(bobRow).toContainText('Member');
    await bobRow.getByRole('button', { name: 'Make admin' }).click();
    await expect(bobRow).toContainText('Admin');
    await bobRow.getByRole('button', { name: 'Make member' }).click();
    await expect(bobRow).toContainText('Member');
    await bobRow.getByRole('button', { name: `Remove ${USERS.bob.name}` }).click();
    const confirm = page.getByRole('dialog', {
      name: `Remove ${USERS.bob.name}?`,
    });
    await confirm.getByRole('button', { name: 'Remove' }).click();
    await expect(settings.members).toHaveCount(1);

    // Removing bob regenerated the join link: his old link no longer works.
    await expect(settings.joinLink).not.toHaveValue(joinLink);
    const afterRemoval = await settings.joinLink.inputValue();

    // An admin can also regenerate it by hand.
    await settings.regenerate.click();
    await expect(settings.joinLink).not.toHaveValue(afterRemoval);
    await bob.goto(new URL(joinLink).pathname);
    await expect(bob.getByRole('heading', { level: 1 })).toHaveText(
      'This join link no longer works',
    );
    await bob.context().close();

    // Delete the team: alice is back to onboarding.
    await settings.deleteTeam('E2E Renamed');
    await expect(new OnboardingPage(page).heading).toBeVisible();
  });
});
