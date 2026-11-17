import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';

import {
  createTeam,
  deleteTeam,
  joinTeam,
  meetingAction,
  meetingState,
  startMeeting,
  writeReflection,
} from '../api';
import { watchCsp } from '../csp';
import { PASSWORD, USERS } from '../fixtures';
import { AppShell } from '../pages/AppShell';
import { GroupPage } from '../pages/GroupPage';
import { SignInPage } from '../pages/SignInPage';

const TEAM = 'Group E2E';
const OMAR = USERS.omar.username;
const PIA = USERS.pia.username;

const FLAKY = 'Flaky e2e tests blocked merges';
const SLOW = 'CI takes 25 minutes';
const QUARANTINE = 'Quarantine flaky tests';
const TRAIN = 'Release train shipped on time';
const THANKS = 'Thanks for covering the incident';

// This spec owns omar and pia, so it never races the other specs.
test.describe.configure({ mode: 'serial' });

async function signIn(page: Page, user: keyof typeof USERS): Promise<void> {
  const signInPage = new SignInPage(page);
  await signInPage.goto();
  await signInPage.signIn(USERS[user].username, PASSWORD);
  await expect(new AppShell(page).currentUser).toHaveText(USERS[user].name);
}

test.describe('group', () => {
  let teamId: string;
  let meetingId: string;
  let cspViolations: string[][];
  let contexts: BrowserContext[];
  /** Omar, the facilitator (team admin). */
  let omar: GroupPage;

  /** Another participant in their own browser, in the meeting room. */
  async function open(
    browser: Browser,
    user: keyof typeof USERS,
    colorScheme: 'light' | 'dark' = 'light',
  ): Promise<GroupPage> {
    const context = await browser.newContext({
      ignoreHTTPSErrors: true,
      colorScheme,
    });
    contexts.push(context);
    const page = await context.newPage();
    cspViolations.push(await watchCsp(page));
    await signIn(page, user);
    const room = new GroupPage(page);
    await room.goto(teamId, meetingId);
    return room;
  }

  test.beforeEach(async ({ page }) => {
    contexts = [];
    cspViolations = [];
    // Omar facilitates, Pia joined; both wrote cards, and the meeting is in Group.
    teamId = await createTeam(OMAR, TEAM);
    await joinTeam(OMAR, teamId, PIA);
    meetingId = await startMeeting(OMAR, teamId);
    await meetingAction(PIA, meetingId, 'join');
    await meetingAction(OMAR, meetingId, 'phase/next');
    await writeReflection(OMAR, meetingId, 'plus', `<p>${TRAIN}</p>`);
    await writeReflection(PIA, meetingId, 'delta', `<p>${FLAKY}</p>`);
    await writeReflection(OMAR, meetingId, 'delta', `<p>${SLOW}</p>`);
    await writeReflection(PIA, meetingId, 'ideas', `<p>${QUARANTINE}</p>`);
    await writeReflection(PIA, meetingId, 'thanks', `<p>${THANKS}</p>`);
    await meetingAction(OMAR, meetingId, 'phase/next');
    cspViolations.push(await watchCsp(page));
    await signIn(page, 'omar');
    omar = new GroupPage(page);
    await omar.goto(teamId, meetingId);
  });

  test.afterEach(async () => {
    for (const context of contexts) await context.close();
    await deleteTeam(OMAR, teamId);
    // The room works within the app's own Content Security Policy.
    expect(cspViolations.flat()).toEqual([]);
  });

  test('everyone sees every card, anonymously, in its column', async () => {
    await expect(omar.column('Plus').getByRole('article')).toHaveText([/Release train/]);
    await expect(omar.column('Delta').getByRole('article')).toHaveCount(2);
    await expect(omar.column('Ideas')).toContainText(QUARANTINE);
    await expect(omar.ready).toBeVisible();
    await expect(omar.nextVote).toBeVisible();
  });

  test('participants group cards by dragging, live for everyone', async ({ browser }) => {
    const pia = await open(browser, 'pia');

    // An idea dropped on a delta card: a group in Delta, at that card's place.
    await omar.drag(QUARANTINE, omar.card(SLOW));
    for (const room of [omar, pia]) {
      const group = room.group('Untitled group');
      await expect(group).toBeVisible();
      await expect(group).toContainText('2 cards');
      await expect(room.column('Delta').getByTestId('group')).toHaveCount(1);
      await expect(room.column('Ideas').getByRole('article')).toHaveCount(0);
    }

    // A card dropped on the group joins it.
    await pia.drag(FLAKY, pia.group('Untitled group'));
    await expect(omar.group('Untitled group')).toContainText('3 cards');

    // Pia names it; Omar sees the name.
    await pia.rename(pia.group('Untitled group'), 'Flaky CI');
    await expect(omar.groupName(omar.group('Flaky CI'))).toHaveValue('Flaky CI');

    // Dragged out onto a column, a card leaves; a group of one dissolves.
    await omar.expand(omar.group('Flaky CI'));
    await omar.drag(FLAKY, omar.column('Thanks'));
    await expect(pia.group('Flaky CI')).toContainText('2 cards');
    await expect(pia.column('Delta').getByRole('article').filter({ hasText: FLAKY })).toHaveCount(
      1,
    );
    await omar.drag(QUARANTINE, omar.column('Ideas'));
    for (const room of [omar, pia]) {
      await expect(room.page.getByTestId('group')).toHaveCount(0);
      await expect(room.column('Ideas')).toContainText(QUARANTINE);
    }
    const state = await meetingState(OMAR, meetingId);
    expect(state.groups).toEqual([]);
    expect(state.reflections.every((r) => r.groupId === null)).toBe(true);
  });

  test('everyone sees who is dragging a card, which nobody else can pick up', async ({
    browser,
  }) => {
    const pia = await open(browser, 'pia');
    await pia.pickUp(TRAIN);
    await expect(omar.badge(TRAIN)).toHaveText(/Pia is moving/);
    await expect(omar.moveTo(TRAIN)).toBeDisabled();

    // Escape puts it back, nowhere new.
    await pia.page.keyboard.press('Escape');
    await expect(pia.ghost).toHaveCount(0);
    await pia.page.mouse.up();
    await expect(omar.badge(TRAIN)).toHaveCount(0);
    await expect(omar.moveTo(TRAIN)).toBeEnabled();
    await expect(omar.column('Plus')).toContainText(TRAIN);
  });

  test('cards are grouped from the keyboard with Move to…', async ({ browser }) => {
    const pia = await open(browser, 'pia');
    await omar.moveWithKeyboard(QUARANTINE, SLOW);
    const group = omar.group('Untitled group');
    // The group opens on the moved card, which keeps the focus.
    await expect(group.getByRole('button', { name: 'Collapse group' })).toBeVisible();
    await expect(omar.moveTo(QUARANTINE)).toBeFocused();
    await expect(pia.group('Untitled group')).toContainText('2 cards');

    await omar.moveWithKeyboard(TRAIN, 'Untitled group');
    await expect(pia.group('Untitled group')).toContainText('3 cards');

    await omar.moveWithKeyboard(QUARANTINE, 'Remove from group');
    await expect(pia.group('Untitled group')).toContainText('2 cards');
    await expect(pia.column('Ideas')).toContainText(QUARANTINE);
  });

  test('the Group screen works in the dark theme', async ({ browser }) => {
    const pia = await open(browser, 'pia', 'dark');
    await omar.moveWithKeyboard(QUARANTINE, SLOW);

    // Wells, cards and the open group come from the dark tokens.
    await expect(pia.column('Delta')).toHaveCSS('background-color', 'rgb(23, 19, 42)');
    await expect(omar.column('Delta')).toHaveCSS('background-color', 'rgb(228, 224, 238)');
    await expect(pia.card(TRAIN)).toHaveCSS('background-color', 'rgb(53, 43, 85)');
    const group = pia.group('Untitled group');
    await pia.expand(group);
    await expect(group.locator('.group__panel')).toHaveCSS('background-color', 'rgb(58, 47, 94)');
  });
});
