import { expect, test, type Browser, type Page } from '@playwright/test';

import { resetAccount, seedTask, userIdOf } from '../api';
import { PASSWORD, USERS } from '../fixtures';
import { AccountPage } from '../pages/AccountPage';
import { AppShell } from '../pages/AppShell';
import { OnboardingPage } from '../pages/OnboardingPage';
import { SignInPage } from '../pages/SignInPage';
import { TasksPage } from '../pages/TasksPage';
import { TeamSettingsPage } from '../pages/TeamSettingsPage';

const TEAM = 'Tasks E2E';
const ERIN = USERS.erin.username;
const FINN = USERS.finn.username;

const PIN = 'Pin it';
const STUCK = 'Erin stuck item';
const FUTURE = ['Future one', 'Future two', 'Future three'];

// This spec owns the erin and finn accounts, so it never races the other specs.
test.describe.configure({ mode: 'serial' });

async function signIn(page: Page, user: keyof typeof USERS): Promise<AppShell> {
  const signInPage = new SignInPage(page);
  await signInPage.goto();
  await signInPage.signIn(USERS[user].username, PASSWORD);
  const shell = new AppShell(page);
  await expect(shell.currentUser).toHaveText(USERS[user].name);
  return shell;
}

test.describe('tasks board', () => {
  let teamId: string;

  test.beforeEach(async ({ page }) => {
    await resetAccount(ERIN);
    await resetAccount(FINN);
    // Erin has no team after the reset: create it from onboarding.
    await signIn(page, 'erin');
    teamId = await new OnboardingPage(page).createTeamNamed(TEAM);
  });

  test.afterEach(async () => {
    await resetAccount(ERIN);
    await resetAccount(FINN);
  });

  /** Finn joins through the team's join link, in his own browser session. */
  async function finnJoins(page: Page, browser: Browser): Promise<Page> {
    const settings = new TeamSettingsPage(page);
    await settings.goto(teamId);
    await expect(settings.joinSwitch).toHaveAttribute('aria-checked', 'true');
    const joinLink = await settings.joinLink.inputValue();

    const finn = await (await browser.newContext()).newPage();
    await signIn(finn, 'finn');
    await finn.goto(new URL(joinLink).pathname);
    await expect(finn.getByRole('heading', { level: 1 })).toHaveText(`You've joined ${TEAM}`);
    return finn;
  }

  test('board: drag, reorder, move, filter, edit and delete', async ({ page, browser }) => {
    const finn = await finnJoins(page, browser);
    const finnId = await userIdOf(FINN);

    // Four tasks (plus two more in Future, so a column can be reordered).
    await seedTask(ERIN, teamId, {
      status: 'active',
      content: `<p>${PIN} <span data-type="mention" data-id="${finnId}" data-label="${USERS.finn.name}">@${USERS.finn.name}</span></p>`,
    });
    await seedTask(ERIN, teamId, {
      status: 'future',
      content: `<p>${FUTURE[0]}</p>`,
      authorId: finnId,
    });
    for (const text of FUTURE.slice(1)) {
      await seedTask(ERIN, teamId, { status: 'future', content: `<p>${text}</p>` });
    }
    await seedTask(ERIN, teamId, { status: 'stuck', content: `<p>${STUCK}</p>` });

    // Erin sees all four columns with their counts.
    const tasks = new TasksPage(page);
    await tasks.goto(teamId);
    for (const name of ['Future', 'Stuck', 'Active', 'Done'] as const) {
      await expect(tasks.column(name)).toBeVisible();
    }
    await expect(tasks.count('Future')).toHaveText('3');
    await expect(tasks.count('Stuck')).toHaveText('1');
    await expect(tasks.count('Active')).toHaveText('1');
    await expect(tasks.count('Done')).toHaveText('0');
    await expect(tasks.card(PIN)).toContainText(`@${USERS.finn.name}`);

    // The actions menu of the last card of the tallest column is not clipped.
    await tasks.actions(FUTURE[2]).click();
    await expect(tasks.menu.getByRole('menuitem', { name: 'Edit' })).toBeInViewport();
    await expect(tasks.menu.getByRole('menuitem', { name: 'Delete' })).toBeInViewport();
    await page.keyboard.press('Escape');
    await expect(tasks.menu).toBeHidden();

    // A real drag from Active to Done survives a reload.
    await tasks.dragToColumn(PIN, 'Done');
    await expect(tasks.count('Done')).toHaveText('1');
    await expect(tasks.cards('Done')).toContainText([PIN]);
    await page.reload();
    await expect(tasks.count('Done')).toHaveText('1');
    await expect(tasks.cards('Done')).toContainText([PIN]);
    await expect(tasks.count('Active')).toHaveText('0');

    // A downward drag within Future: the first card goes below the second.
    await expect(tasks.cards('Future')).toContainText(FUTURE);
    await tasks.drag(tasks.card(FUTURE[0]!), tasks.card(FUTURE[1]!), 0.9);
    await expect(tasks.cards('Future')).toContainText([FUTURE[1]!, FUTURE[0]!, FUTURE[2]!]);
    await page.reload();
    await expect(tasks.cards('Future')).toContainText([FUTURE[1]!, FUTURE[0]!, FUTURE[2]!]);

    // Move to… from the keyboard route.
    await tasks.moveWithKeyboard(STUCK, 'Active');
    await expect(tasks.cards('Active')).toContainText([STUCK]);
    await expect(tasks.count('Stuck')).toHaveText('0');
    await page.reload();
    await expect(tasks.cards('Active')).toContainText([STUCK]);

    // Finn's "My tasks" shows only the card that mentions him.
    const finnTasks = new TasksPage(finn);
    await finnTasks.goto(teamId);
    await expect(finn.getByRole('article')).toHaveCount(5);
    await finnTasks.filterMine();
    await expect(finn.getByRole('article')).toHaveCount(1);
    await expect(finnTasks.cards('Done')).toContainText([PIN]);
    await expect(finn).toHaveURL(new RegExp(`mentioned=`));
    await finn.context().close();

    // Erin edits her card, and the new text persists.
    await tasks.edit(STUCK, ' (edited)');
    await expect(tasks.card(STUCK)).toContainText(`${STUCK} (edited)`);
    await page.reload();
    await expect(tasks.card(STUCK)).toContainText(`${STUCK} (edited)`);

    // …then deletes it.
    await tasks.delete(STUCK);
    await expect(tasks.card(STUCK)).toHaveCount(0);
    await page.reload();
    await expect(tasks.heading).toBeVisible();
    await expect(tasks.card(STUCK)).toHaveCount(0);
    await expect(page.getByRole('article')).toHaveCount(4);
  });

  for (const theme of ['Light', 'Dark'] as const) {
    test(`mobile: no horizontal scroll at 390 px in the ${theme.toLowerCase()} theme`, async ({
      page,
    }) => {
      for (const [status, text] of [
        ['future', FUTURE[0]!],
        ['stuck', STUCK],
        ['active', PIN],
        ['done', 'Shipped it'],
      ] as const) {
        await seedTask(ERIN, teamId, { status, content: `<p>${text}</p>` });
      }
      const account = new AccountPage(page);
      await account.goto();
      await account.chooseTheme(theme);
      await page.setViewportSize({ width: 390, height: 844 });
      await expect(page.locator('html')).toHaveAttribute('data-theme', theme.toLowerCase());

      const tasks = new TasksPage(page);
      await tasks.goto(teamId);
      await expect(tasks.card(PIN)).toBeVisible();
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
