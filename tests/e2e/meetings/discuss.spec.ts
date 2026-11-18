import {
  expect,
  test,
  type Browser,
  type BrowserContext,
  type Page,
  type WebSocketRoute,
} from '@playwright/test';

import {
  castVote,
  createTeam,
  deleteTeam,
  groupCards,
  joinTeam,
  meetingAction,
  meetingState,
  react,
  renameGroup,
  setDiscussGroup,
  setRole,
  startMeeting,
  userIdOf,
  writeReflection,
} from '../api';
import { watchCsp } from '../csp';
import { PASSWORD, USERS } from '../fixtures';
import { channelSubscribed } from '../realtime';
import { AppShell } from '../pages/AppShell';
import { DiscussPage } from '../pages/DiscussPage';
import { SignInPage } from '../pages/SignInPage';
import { TasksPage } from '../pages/TasksPage';

const TEAM = 'Discuss E2E';
const UMA = USERS.uma.username;
const VERA = USERS.vera.username;
const WALT = USERS.walt.username;
const XENA = USERS.xena.username;

const FLAKY = 'Flaky e2e tests blocked merges';
const SLOW = 'CI takes 25 minutes';
const PAGER = 'Pager alerts at night were noise';
const THANKS = 'Thanks Walt for covering the incident';
/** Never grouped: a group of its own, named by its text. */
const DEMO = 'Demo slot on Fridays';

const CI = 'Flaky CI';
const ONCALL = 'On-call';

// This spec owns uma, vera, walt and xena, so it never races the other specs.
test.describe.configure({ mode: 'serial' });

async function signIn(page: Page, user: keyof typeof USERS): Promise<void> {
  const signInPage = new SignInPage(page);
  await signInPage.goto();
  await signInPage.signIn(USERS[user].username, PASSWORD);
  await expect(new AppShell(page).currentUser).toHaveText(USERS[user].name);
}

async function cardId(meetingId: string, text: string): Promise<string> {
  const state = await meetingState(UMA, meetingId);
  return state.reflections.find((r) => r.content.includes(text))!.id;
}

async function groupOf(meetingId: string, text: string): Promise<string> {
  const state = await meetingState(UMA, meetingId);
  return state.reflections.find((r) => r.content.includes(text))!.groupId!;
}

test.describe('discuss', () => {
  let teamId: string;
  let meetingId: string;
  let cspViolations: string[][];
  let contexts: BrowserContext[];
  /** Uma, the facilitator (team admin), in the test's own page. */
  let uma: DiscussPage;

  interface OpenOptions {
    colorScheme?: 'light' | 'dark';
    viewport?: { width: number; height: number };
    /** Wait until the room's channel is live (subscribed, and re-read after). */
    live?: boolean;
  }

  /** Someone in their own browser, in the meeting room. */
  async function open(
    browser: Browser,
    user: keyof typeof USERS,
    options: OpenOptions = {},
  ): Promise<DiscussPage> {
    const context = await browser.newContext({
      ignoreHTTPSErrors: true,
      colorScheme: options.colorScheme ?? 'light',
      ...(options.viewport ? { viewport: options.viewport } : {}),
    });
    contexts.push(context);
    const page = await context.newPage();
    cspViolations.push(await watchCsp(page));
    await signIn(page, user);
    const room = new DiscussPage(page);
    await live(room, () => room.goto(teamId, meetingId));
    return room;
  }

  /** Runs `navigation` and waits until the room is live: events reach it from now on. */
  async function live(room: DiscussPage, navigation: () => Promise<void>): Promise<void> {
    const subscribed = channelSubscribed(room.page, `meeting:${meetingId}`);
    const refreshed = room.page.waitForResponse(
      (r) => r.request().method() === 'GET' && new URL(r.url()).pathname.endsWith('/state'),
    );
    await navigation();
    await Promise.all([subscribed, refreshed]);
  }

  /** The meeting moves on to Discuss (through the API). */
  async function toDiscuss(): Promise<void> {
    await meetingAction(UMA, meetingId, 'phase/next');
    await live(uma, () => uma.goto(teamId, meetingId));
  }

  test.beforeEach(async ({ page }) => {
    contexts = [];
    cspViolations = [];
    // Uma facilitates; Vera, Walt and Xena (a second admin) take part.
    teamId = await createTeam(UMA, TEAM);
    for (const user of [VERA, WALT, XENA]) await joinTeam(UMA, teamId, user);
    await setRole(UMA, teamId, await userIdOf(XENA), 'admin');
    meetingId = await startMeeting(UMA, teamId);
    for (const user of [VERA, WALT, XENA]) await meetingAction(user, meetingId, 'join');
    await meetingAction(UMA, meetingId, 'phase/next');
    await writeReflection(VERA, meetingId, 'delta', `<p>${FLAKY}</p>`);
    await writeReflection(WALT, meetingId, 'delta', `<p>${SLOW}</p>`);
    await writeReflection(XENA, meetingId, 'delta', `<p>${PAGER}</p>`);
    await writeReflection(VERA, meetingId, 'thanks', `<p>${THANKS}</p>`);
    await writeReflection(WALT, meetingId, 'ideas', `<p>${DEMO}</p>`);
    // Group: "Flaky CI" and "On-call"; the demo idea stays on its own.
    await meetingAction(UMA, meetingId, 'phase/next');
    const ci = await groupCards(
      VERA,
      meetingId,
      await cardId(meetingId, FLAKY),
      await cardId(meetingId, SLOW),
    );
    await renameGroup(VERA, ci, CI);
    const oncall = await groupCards(
      WALT,
      meetingId,
      await cardId(meetingId, PAGER),
      await cardId(meetingId, THANKS),
    );
    await renameGroup(WALT, oncall, ONCALL);
    // Vote: Flaky CI 6, On-call 3, the demo idea 1.
    await meetingAction(UMA, meetingId, 'phase/next');
    const demo = await groupOf(meetingId, DEMO);
    for (const [user, group, times] of [
      [VERA, ci, 3],
      [WALT, ci, 3],
      [UMA, oncall, 3],
      [XENA, demo, 1],
    ] as const) {
      for (let i = 0; i < times; i++) {
        expect(await castVote(user, group, 1)).toEqual({
          status: 200,
          code: null,
        });
      }
    }
    cspViolations.push(await watchCsp(page));
    await signIn(page, 'uma');
    uma = new DiscussPage(page);
  });

  test.afterEach(async () => {
    // Every step runs even if one fails, so the team never outlives the test.
    const failures: unknown[] = [];
    for (const context of contexts) await context.close().catch((e: unknown) => failures.push(e));
    await deleteTeam(UMA, teamId).catch((e: unknown) => failures.push(e));
    if (failures.length > 0) {
      throw new AggregateError(failures, `discuss cleanup: ${failures.length} step(s) failed`);
    }
    // The room works within the app's own Content Security Policy.
    expect(cspViolations.flat()).toEqual([]);
  });

  test('the facilitator opens Discuss and everyone sees the groups ranked by votes', async ({
    browser,
  }) => {
    await uma.page.goto(`/teams/${teamId}/meetings/${meetingId}`);
    await expect(uma.phase).toHaveText('Vote');
    const vera = await (async () => {
      const context = await browser.newContext({ ignoreHTTPSErrors: true });
      contexts.push(context);
      const page = await context.newPage();
      cspViolations.push(await watchCsp(page));
      await signIn(page, 'vera');
      const room = new DiscussPage(page);
      const subscribed = channelSubscribed(page, `meeting:${meetingId}`);
      await page.goto(`/teams/${teamId}/meetings/${meetingId}`);
      await expect(room.phase).toHaveText('Vote');
      await subscribed;
      return room;
    })();

    await uma.footer.getByRole('button', { name: 'Next: Discuss' }).click();
    for (const room of [uma, vera]) {
      await expect(room.phase).toHaveText('Discuss');
      await expect(room.page.getByText('Phase 5 of 5')).toBeVisible();
      // Groups by votes, with their totals.
      const entries = room.progress.getByRole('button');
      await expect(entries).toHaveCount(3);
      await expect(entries.nth(0)).toContainText(CI);
      await expect(entries.nth(1)).toContainText(ONCALL);
      await expect(entries.nth(2)).toContainText(DEMO);
      await expect(entries.nth(0).getByLabel('6 votes')).toHaveText('6');
      await expect(entries.nth(1).getByLabel('3 votes')).toHaveText('3');
      await expect(entries.nth(2).getByLabel('1 vote')).toHaveText('1');
      await expect(entries.nth(0)).toContainText('Now');
      await expect(entries.nth(1)).toContainText('Next');
      // The current group, with every card and its column.
      await expect(room.groupTitle).toHaveText(CI);
      await expect(room.groupOf(1, 3)).toBeVisible();
      await expect(room.votes).toHaveText('6 votes');
      await expect(room.card(FLAKY)).toBeVisible();
      await expect(room.card(SLOW)).toBeVisible();
      await expect(room.card(FLAKY).getByRole('img', { name: 'Delta' })).toBeVisible();
      await expect(room.group.getByRole('article')).toHaveCount(2);
      await expect(room.footer).toContainText('Group 1 of 3 · everyone follows the facilitator');
      // No Ready in Discuss.
      await expect(room.footer.getByRole('button', { name: /ready/i })).toHaveCount(0);
    }
    // Only the facilitator moves the group.
    await expect(uma.nextGroup).toBeVisible();
    await expect(uma.previousGroup).toBeDisabled();
    await expect(vera.nextGroup).toHaveCount(0);
    await expect(vera.previousGroup).toHaveCount(0);
  });

  test('the facilitator moves everyone through the groups, back and forth', async ({ browser }) => {
    await toDiscuss();
    const vera = await open(browser, 'vera');
    const walt = await open(browser, 'walt');

    await uma.nextGroup.click();
    for (const room of [uma, vera, walt]) {
      await expect(room.groupTitle).toHaveText(ONCALL);
      await expect(room.groupOf(2, 3)).toBeVisible();
      await expect(room.votes).toHaveText('3 votes');
      await expect(room.card(PAGER)).toBeVisible();
      await expect(room.card(THANKS).getByRole('img', { name: 'Thanks' })).toBeVisible();
      await expect(room.entry(CI).getByLabel('done')).toBeVisible();
      await expect(room.entry(ONCALL)).toContainText('Now');
      await expect(room.entry(DEMO)).toContainText('Next');
      await expect(room.footer).toContainText('Group 2 of 3');
    }

    await uma.nextGroup.click();
    for (const room of [uma, vera, walt]) {
      await expect(room.groupTitle).toHaveText(DEMO);
      await expect(room.groupOf(3, 3)).toBeVisible();
      await expect(room.votes).toHaveText('1 vote');
    }
    // The last group: End meeting instead of Next group.
    await expect(uma.nextGroup).toHaveCount(0);
    await expect(uma.endMeeting).toBeVisible();

    await uma.previousGroup.click();
    for (const room of [uma, vera, walt]) await expect(room.groupTitle).toHaveText(ONCALL);
    await uma.previousGroup.click();
    for (const room of [uma, vera, walt]) await expect(room.groupTitle).toHaveText(CI);
    await expect(uma.previousGroup).toBeDisabled();
    // The server agrees.
    expect((await meetingState(VERA, meetingId)).discussGroupId).toBe(
      await groupOf(meetingId, FLAKY),
    );
  });

  test('a participant previews another group, read-only, without moving anyone', async ({
    browser,
  }) => {
    await toDiscuss();
    const vera = await open(browser, 'vera');
    const walt = await open(browser, 'walt');

    await vera.entry(DEMO).click();
    await expect(vera.groupTitle).toHaveText(DEMO);
    await expect(vera.groupOf(3, 3)).toBeVisible();
    await expect(vera.entry(DEMO)).toHaveAttribute('aria-pressed', 'true');
    await expect(vera.preview).toContainText('Everyone else is on group 1');
    // Read-only: no new reactions, no task composer.
    await expect(vera.addReaction(DEMO)).toHaveCount(0);
    await expect(vera.taskEditor).toHaveCount(0);
    // Only a facilitator can make it the current group.
    await expect(vera.discussThis).toHaveCount(0);
    // Nobody else moved, and "Now" stays on the current group.
    await expect(vera.entry(CI)).toContainText('Now');
    for (const room of [uma, walt]) {
      await expect(room.groupTitle).toHaveText(CI);
      await expect(room.preview).toHaveCount(0);
    }

    // Back and forth, freely.
    await vera.entry(ONCALL).click();
    await expect(vera.groupTitle).toHaveText(ONCALL);
    await vera.backToCurrent.click();
    await expect(vera.groupTitle).toHaveText(CI);
    await expect(vera.preview).toHaveCount(0);
    await expect(vera.addReaction(FLAKY)).toBeVisible();
    await expect(vera.taskEditor).toBeVisible();

    // A preview is local: a reload is back on the current group.
    await vera.entry(DEMO).click();
    await expect(vera.groupTitle).toHaveText(DEMO);
    await vera.page.reload();
    await expect(vera.phase).toHaveText('Discuss');
    await expect(vera.groupTitle).toHaveText(CI);

    // The server refuses a reaction or a task on a group that is not current.
    const refused = await react(VERA, await cardId(meetingId, DEMO), '👍');
    expect(refused).toMatchObject({ status: 409, code: 'not_current_group' });
  });

  test('when the facilitator changes group, everyone previewing jumps to it', async ({
    browser,
  }) => {
    await toDiscuss();
    const vera = await open(browser, 'vera');
    const walt = await open(browser, 'walt');
    await vera.entry(DEMO).click();
    await expect(vera.groupTitle).toHaveText(DEMO);
    await walt.entry(CI).click(); // the current group: no preview
    await expect(walt.preview).toHaveCount(0);
    await walt.entry(DEMO).click();
    await expect(walt.preview).toBeVisible();

    await uma.nextGroup.click();
    for (const room of [vera, walt, uma]) {
      await expect(room.groupTitle).toHaveText(ONCALL);
      await expect(room.preview).toHaveCount(0);
      await expect(room.entry(ONCALL)).toContainText('Now');
    }
    // Now on the current group again, they can react and write tasks.
    await expect(vera.addReaction(PAGER)).toBeVisible();
    await expect(walt.taskEditor).toBeVisible();

    // A change made through the API reaches the rooms the same way.
    await vera.entry(CI).click();
    await setDiscussGroup(UMA, meetingId, await groupOf(meetingId, DEMO));
    for (const room of [vera, walt, uma]) await expect(room.groupTitle).toHaveText(DEMO);
    await expect(vera.preview).toHaveCount(0);
  });

  test('the facilitator previews a group and makes it the one everyone discusses', async ({
    browser,
  }) => {
    await toDiscuss();
    const vera = await open(browser, 'vera');

    await uma.entry(DEMO).click();
    await expect(uma.groupTitle).toHaveText(DEMO);
    await expect(uma.preview).toBeVisible();
    // Previewing moves nobody, the facilitator included.
    await expect(vera.groupTitle).toHaveText(CI);
    await expect(uma.footer).toContainText('Group 1 of 3');

    await uma.discussThis.click();
    for (const room of [uma, vera]) {
      await expect(room.groupTitle).toHaveText(DEMO);
      await expect(room.preview).toHaveCount(0);
      await expect(room.entry(DEMO)).toContainText('Now');
      await expect(room.footer).toContainText('Group 3 of 3');
    }
    await expect(uma.endMeeting).toBeVisible();
  });

  test('reactions: several emoji per card, counts live for everyone, a toggle', async ({
    browser,
  }) => {
    await toDiscuss();
    const vera = await open(browser, 'vera');
    const walt = await open(browser, 'walt');

    // Vera reacts from the picker's first row.
    await vera.openPicker(FLAKY);
    await expect(vera.addReaction(FLAKY)).toHaveAttribute('aria-expanded', 'true');
    await vera.saved('PUT', vera.quickReactions.filter({ hasText: '👍' }).click());
    await expect(vera.picker).toBeHidden();
    await expect(vera.chip(FLAKY, '👍')).toHaveAccessibleName('👍 reaction, 1');
    await expect(vera.chip(FLAKY, '👍')).toHaveAttribute('aria-pressed', 'true');
    // Everyone sees it live, as not theirs.
    for (const room of [walt, uma]) {
      await expect(room.chip(FLAKY, '👍')).toHaveAccessibleName('👍 reaction, 1');
      await expect(room.chip(FLAKY, '👍')).toHaveAttribute('aria-pressed', 'false');
    }

    // Walt adds his to the same emoji: one each.
    await walt.toggle(FLAKY, '👍');
    for (const room of [vera, walt, uma]) {
      await expect(room.chip(FLAKY, '👍')).toHaveAccessibleName('👍 reaction, 2');
    }
    await expect(walt.chip(FLAKY, '👍')).toHaveAttribute('aria-pressed', 'true');
    await expect(uma.chip(FLAKY, '👍')).toHaveAttribute('aria-pressed', 'false');

    // Several different emoji on one card, and on another card.
    await vera.reactFromPicker(FLAKY, 'Grimacing face');
    await vera.reactFromPicker(SLOW, 'Hundred points');
    await expect(walt.chip(FLAKY, '😬')).toHaveAccessibleName('😬 reaction, 1');
    await expect(walt.chip(SLOW, '💯')).toHaveAccessibleName('💯 reaction, 1');
    await expect(walt.card(FLAKY).getByTestId('reaction-chip')).toHaveCount(2);
    // In the picker, the viewer's own emoji on the card are pressed.
    await vera.openPicker(FLAKY);
    await expect(vera.quickReactions.filter({ hasText: '👍' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await vera.page.keyboard.press('Escape');
    await expect(vera.picker).toBeHidden();

    // Toggling takes Vera's back; Walt's stays.
    await vera.toggle(FLAKY, '👍');
    for (const room of [vera, walt, uma]) {
      await expect(room.chip(FLAKY, '👍')).toHaveAccessibleName('👍 reaction, 1');
    }
    await expect(vera.chip(FLAKY, '👍')).toHaveAttribute('aria-pressed', 'false');
    await expect(walt.chip(FLAKY, '👍')).toHaveAttribute('aria-pressed', 'true');
    // Walt takes his back too: the emoji is gone.
    await walt.toggle(FLAKY, '👍');
    for (const room of [vera, walt, uma]) await expect(room.chip(FLAKY, '👍')).toHaveCount(0);

    // The same in a second tab of Vera's: her own reactions follow her.
    const tab = new DiscussPage(await vera.page.context().newPage());
    cspViolations.push(await watchCsp(tab.page));
    await live(tab, () => tab.goto(teamId, meetingId));
    await expect(tab.chip(FLAKY, '😬')).toHaveAttribute('aria-pressed', 'true');
    await vera.toggle(SLOW, '💯');
    await expect(tab.chip(SLOW, '💯')).toHaveCount(0);
    await tab.toggle(FLAKY, '😬');
    await expect(vera.chip(FLAKY, '😬')).toHaveCount(0);

    // Reactions stay after a reload.
    await walt.reactFromPicker(SLOW, 'Party popper');
    await walt.page.reload();
    await expect(walt.phase).toHaveText('Discuss');
    await expect(walt.chip(SLOW, '🎉')).toHaveAccessibleName('🎉 reaction, 1');
    await expect(walt.chip(SLOW, '🎉')).toHaveAttribute('aria-pressed', 'true');
    const state = await meetingState(WALT, meetingId);
    expect(state.reactions).toEqual([
      {
        reflectionId: await cardId(meetingId, SLOW),
        emoji: '🎉',
        count: 1,
        mine: true,
      },
    ]);
  });

  test('the first row of the picker holds the emoji the user reacts with most', async ({
    browser,
  }) => {
    await toDiscuss();
    const vera = await open(browser, 'vera');
    const walt = await open(browser, 'walt');

    // A fresh user gets the defaults.
    await vera.openPicker(FLAKY);
    await expect(vera.quickReactions).toHaveText(['👍', '❤️', '😂', '🎉', '😮', '😬', '💯', '🙏']);
    await expect(vera.picker.getByText('Emoji you react with show up here.')).toBeVisible();

    // Vera reacts with a rocket (search), twice, and a unicorn once.
    await vera.search.fill('rocket');
    await vera.saved(
      'PUT',
      vera.picker.getByRole('button', { name: 'emoji Rocket', exact: true }).click(),
    );
    await vera.openPicker(SLOW);
    // The rocket is now in Recently used.
    await vera.saved(
      'PUT',
      vera.picker
        .getByRole('group', { name: 'Recently used' })
        .getByRole('button', { name: 'emoji Rocket', exact: true })
        .click(),
    );
    await vera.openPicker(SLOW);
    await vera.search.fill('unicorn');
    await vera.saved(
      'PUT',
      vera.picker.getByRole('button', { name: 'emoji Unicorn', exact: true }).click(),
    );

    await vera.openPicker(FLAKY);
    await expect(vera.quickReactions).toHaveText(['🚀', '🦄', '👍', '❤️', '😂', '🎉', '😮', '😬']);
    await expect(vera.quickReactions.first()).toHaveAccessibleName('Rocket');
    // Her own rocket on this card shows pressed.
    await expect(vera.quickReactions.first()).toHaveAttribute('aria-pressed', 'true');
    await expect(
      vera.picker.getByRole('group', { name: 'Recently used' }).getByRole('button'),
    ).toHaveText(['🦄', '🚀']);
    await vera.page.keyboard.press('Escape');

    // Kept in this browser, for this user: it survives a reload.
    const veraId = await userIdOf(VERA);
    const stored = await vera.page.evaluate(
      (key) => localStorage.getItem(key),
      `retro.emojiUsage.${veraId}`,
    );
    expect(JSON.parse(stored!)).toEqual({
      counts: { '🚀': 2, '🦄': 1 },
      recent: ['🦄', '🚀'],
    });
    await vera.page.reload();
    await expect(vera.phase).toHaveText('Discuss');
    await vera.openPicker(SLOW);
    await expect(vera.quickReactions.first()).toHaveText('🚀');

    // Walt, in his own browser, still has the defaults: usage is per user.
    await walt.openPicker(FLAKY);
    await expect(walt.quickReactions.first()).toHaveText('👍');
    await expect(walt.quickReactions.nth(1)).toHaveText('❤️');
  });

  test('the picker: search, categories, keyboard and Esc', async ({ browser }) => {
    await toDiscuss();
    const vera = await open(browser, 'vera');

    await vera.openPicker(FLAKY);
    await expect(vera.search).toBeFocused();
    const tabs = vera.picker.getByRole('tab');
    await expect(tabs).toHaveCount(7);
    await expect(tabs.first()).toHaveAccessibleName('Recently used');
    await expect(tabs.first()).toHaveAttribute('aria-selected', 'true');

    // A category tab scrolls to its emoji.
    await vera.picker.getByRole('tab', { name: 'Food' }).click();
    await expect(vera.picker.getByRole('tab', { name: 'Food' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await expect(
      vera.picker.getByRole('button', { name: 'emoji Grapes', exact: true }),
    ).toBeInViewport();

    // Arrow keys move between emoji; the footer previews the focused one.
    const grapes = vera.picker.getByRole('button', { name: 'emoji Grapes', exact: true });
    await grapes.focus();
    await expect(vera.picker).toContainText(':grapes: · Enter to react');
    await vera.page.keyboard.press('ArrowRight');
    await expect(grapes).not.toBeFocused();
    await vera.page.keyboard.press('ArrowLeft');
    await expect(grapes).toBeFocused();

    // A search narrows the grid; nothing found says so.
    await vera.search.fill('zzzqqq');
    await expect(vera.picker).toContainText('No emoji match "zzzqqq".');
    await vera.search.fill('party pop');
    await expect(
      vera.picker.getByRole('group', { name: 'Search results' }).getByRole('button').first(),
    ).toHaveAccessibleName('emoji Party popper');
    // Enter reacts with the first result.
    await vera.saved('PUT', vera.search.press('Enter'));
    await expect(vera.picker).toBeHidden();
    await expect(vera.chip(FLAKY, '🎉')).toHaveAccessibleName('🎉 reaction, 1');
    // Focus is back on the trigger.
    await expect(vera.addReaction(FLAKY)).toBeFocused();

    // Esc, or a click elsewhere, closes it without reacting.
    await vera.openPicker(SLOW);
    await vera.page.keyboard.press('Escape');
    await expect(vera.picker).toBeHidden();
    await expect(vera.addReaction(SLOW)).toBeFocused();
    await vera.openPicker(SLOW);
    await vera.groupTitle.click();
    await expect(vera.picker).toBeHidden();
    await expect(vera.card(SLOW).getByTestId('reaction-chip')).toHaveCount(0);
  });

  test('tasks: written with a mention, live for everyone, on the board in Active', async ({
    browser,
  }) => {
    await toDiscuss();
    const vera = await open(browser, 'vera');
    const walt = await open(browser, 'walt');
    await expect(vera.tasks).toContainText('Added to Active on the board');
    await expect(vera.tasks).toContainText('No tasks for this group yet.');

    // Vera writes a task with bold text and a mention of Walt.
    await vera.taskEditor.click();
    await vera.page.keyboard.press('ControlOrMeta+b');
    await vera.page.keyboard.type('Quarantine');
    await vera.page.keyboard.press('ControlOrMeta+b');
    await vera.page.keyboard.type(' flaky tests ');
    await vera.page.keyboard.type('@Wal');
    await expect(vera.mentions.getByRole('option', { name: USERS.walt.name })).toBeVisible();
    await vera.page.keyboard.press('Enter');
    const [res] = await Promise.all([
      vera.page.waitForResponse(
        (r) => r.request().method() === 'POST' && new URL(r.url()).pathname.endsWith('/tasks'),
      ),
      vera.addTask.click(),
    ]);
    expect(res.status()).toBe(201);
    // The composer is empty again.
    await expect(vera.taskEditor).toHaveText('');

    for (const room of [vera, walt, uma]) {
      const item = room.taskItems().filter({ hasText: 'Quarantine flaky tests' });
      await expect(item).toBeVisible();
      await expect(item.locator('strong')).toHaveText('Quarantine');
      await expect(item).toContainText(`@${USERS.walt.name}`);
      await expect(item).toContainText(new RegExp(`${USERS.vera.name} · \\d{2}:\\d{2}`));
    }

    // Walt writes a second one; both list in order.
    await walt.writeTask('Split the CI pipeline');
    await expect(vera.taskItems()).toHaveCount(2);
    await expect(vera.taskItems().nth(1)).toContainText('Split the CI pipeline');

    // On the board, in Active, linked to the meeting, mentioning Walt.
    const boardPage = await uma.page.context().newPage();
    cspViolations.push(await watchCsp(boardPage));
    const board = new TasksPage(boardPage);
    await board.goto(teamId);
    await expect(board.cards('Active')).toHaveCount(2);
    await expect(board.card('Quarantine flaky tests')).toContainText(`@${USERS.walt.name}`);
    await expect(board.card('Quarantine flaky tests')).toContainText('Meeting');

    // Tasks belong to their group: the next group starts empty, and the
    // previous one keeps them, read-only.
    await uma.nextGroup.click();
    await expect(vera.groupTitle).toHaveText(ONCALL);
    await expect(vera.taskItems()).toHaveCount(0);
    await vera.entry(CI).click();
    await expect(vera.taskItems()).toHaveCount(2);
    await expect(vera.taskEditor).toHaveCount(0);
  });

  test('on the last group the facilitator ends the meeting for everyone', async ({ browser }) => {
    await toDiscuss();
    const vera = await open(browser, 'vera');
    await uma.nextGroup.click();
    await uma.nextGroup.click();
    await expect(uma.groupTitle).toHaveText(DEMO);
    await expect(vera.groupTitle).toHaveText(DEMO);

    await uma.endMeeting.click();
    await expect(uma.endDialog).toBeVisible();
    await uma.endDialog.getByRole('button', { name: 'End meeting' }).click();
    for (const room of [uma, vera]) await expect(room.ended).toBeVisible();
    // Ended: no more reactions.
    const refused = await react(VERA, await cardId(meetingId, DEMO), '👍');
    expect(refused).toMatchObject({ status: 409, code: 'meeting_ended' });
  });

  test('a team admin takes over facilitation to move the group', async ({ browser }) => {
    await toDiscuss();
    const xena = await open(browser, 'xena');
    const vera = await open(browser, 'vera');
    await expect(xena.nextGroup).toHaveCount(0);

    await xena.adminActions.getByRole('button', { name: 'Take over facilitation' }).click();
    await expect(xena.nextGroup).toBeVisible();
    await expect(uma.nextGroup).toHaveCount(0);
    await xena.nextGroup.click();
    for (const room of [xena, vera, uma]) await expect(room.groupTitle).toHaveText(ONCALL);
    // Uma can no longer move it.
    await uma.entry(DEMO).click();
    await expect(uma.discussThis).toHaveCount(0);
  });

  test('a participant who lost the connection catches up with the current group', async ({
    browser,
  }) => {
    await toDiscuss();
    const context = await browser.newContext({ ignoreHTTPSErrors: true });
    contexts.push(context);
    const page = await context.newPage();
    cspViolations.push(await watchCsp(page));
    let socket: WebSocketRoute | undefined;
    let blocked = false;
    await page.routeWebSocket(/\/api\/v1\/ws\?/, (ws) => {
      if (blocked) {
        void ws.close();
        return;
      }
      socket = ws;
      ws.connectToServer();
    });
    await signIn(page, 'vera');
    const vera = new DiscussPage(page);
    await live(vera, () => vera.goto(teamId, meetingId));
    await vera.entry(DEMO).click();

    // The connection drops; the facilitator moves on meanwhile.
    blocked = true;
    await socket!.close();
    await expect(vera.reconnecting).toBeVisible();
    await uma.nextGroup.click();
    await expect(uma.groupTitle).toHaveText(ONCALL);

    // Back online, Vera's room re-reads the state and follows.
    blocked = false;
    await expect(vera.reconnecting).toBeHidden({ timeout: 15_000 });
    await expect(vera.groupTitle).toHaveText(ONCALL, { timeout: 15_000 });
    await expect(vera.preview).toHaveCount(0);
  });

  test('works in the dark theme and on a narrow screen', async ({ browser }) => {
    await toDiscuss();
    const light = await open(browser, 'walt');
    const dark = await open(browser, 'vera', { colorScheme: 'dark' });
    const background = (room: DiscussPage) =>
      room.group.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(await background(dark)).not.toBe(await background(light));
    await dark.openPicker(FLAKY);
    const pickerBg = await dark.picker.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(pickerBg).not.toBe('rgb(255, 255, 255)');
    await dark.page.keyboard.press('Escape');

    const narrow = await open(browser, 'xena', {
      viewport: { width: 390, height: 844 },
    });
    await expect(narrow.groupTitle).toHaveText(CI);
    await expect(narrow.tasks).toBeVisible();
    await narrow.openPicker(FLAKY);
    await expect(narrow.picker).toBeInViewport({ ratio: 1 });
    await narrow.page.keyboard.press('Escape');
    await expect
      .poll(() =>
        narrow.page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        ),
      )
      .toBe(0);
  });
});
