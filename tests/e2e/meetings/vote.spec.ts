import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';

import {
  castVote,
  createTeam,
  deleteTeam,
  groupCards,
  joinTeam,
  meetingAction,
  meetingState,
  renameGroup,
  startMeeting,
  writeReflection,
} from '../api';
import { watchCsp } from '../csp';
import { channelSubscribed } from '../realtime';
import { PASSWORD, USERS } from '../fixtures';
import { AppShell } from '../pages/AppShell';
import { SignInPage } from '../pages/SignInPage';
import { VotePage } from '../pages/VotePage';

const TEAM = 'Vote E2E';
const QUINN = USERS.quinn.username;
const ROSA = USERS.rosa.username;
const SAMI = USERS.sami.username;
const TESS = USERS.tess.username;

const TRAIN = 'Release train shipped on time';
const PAIRING = 'Smaller PRs made reviews faster';
const FLAKY = 'Flaky e2e tests blocked merges';
const SLOW = 'CI takes 25 minutes';
/** Never grouped: shows as a card of its own, named by its text. */
const DEMO = 'Demo slot on Fridays';
const THANKS = 'Thanks Sami for the pairing';

const SHIPPING = 'Shipping rhythm';
const CI = 'Flaky CI';

// This spec owns quinn, rosa, sami and tess, so it never races the other specs.
test.describe.configure({ mode: 'serial' });

async function signIn(page: Page, user: keyof typeof USERS): Promise<void> {
  const signInPage = new SignInPage(page);
  await signInPage.goto();
  await signInPage.signIn(USERS[user].username, PASSWORD);
  await expect(new AppShell(page).currentUser).toHaveText(USERS[user].name);
}

/** The id of the card whose text includes `text`. */
async function cardId(meetingId: string, text: string): Promise<string> {
  const state = await meetingState(QUINN, meetingId);
  return state.reflections.find((r) => r.content.includes(text))!.id;
}

/** The group a card is in (every card has one from Vote on). */
async function groupOf(meetingId: string, text: string): Promise<string> {
  const state = await meetingState(QUINN, meetingId);
  return state.reflections.find((r) => r.content.includes(text))!.groupId!;
}

test.describe('vote', () => {
  let teamId: string;
  let meetingId: string;
  let cspViolations: string[][];
  let contexts: BrowserContext[];
  /** Quinn, the facilitator (team admin). */
  let quinn: VotePage;

  interface OpenOptions {
    colorScheme?: 'light' | 'dark';
    viewport?: { width: number; height: number };
  }

  /** Someone in their own browser, in the meeting room. */
  async function open(
    browser: Browser,
    user: keyof typeof USERS,
    options: OpenOptions = {},
  ): Promise<VotePage> {
    const context = await browser.newContext({
      ignoreHTTPSErrors: true,
      colorScheme: options.colorScheme ?? 'light',
      ...(options.viewport ? { viewport: options.viewport } : {}),
    });
    contexts.push(context);
    const page = await context.newPage();
    cspViolations.push(await watchCsp(page));
    await signIn(page, user);
    const room = new VotePage(page);
    await room.goto(teamId, meetingId);
    return room;
  }

  test.beforeEach(async ({ page }) => {
    contexts = [];
    cspViolations = [];
    // Quinn facilitates; Rosa and Sami joined; Tess is a member who has not.
    teamId = await createTeam(QUINN, TEAM);
    for (const user of [ROSA, SAMI, TESS]) await joinTeam(QUINN, teamId, user);
    meetingId = await startMeeting(QUINN, teamId);
    await meetingAction(ROSA, meetingId, 'join');
    await meetingAction(SAMI, meetingId, 'join');
    await meetingAction(QUINN, meetingId, 'phase/next');
    await writeReflection(QUINN, meetingId, 'plus', `<p>${TRAIN}</p>`);
    await writeReflection(ROSA, meetingId, 'plus', `<p>${PAIRING}</p>`);
    await writeReflection(ROSA, meetingId, 'delta', `<p>${FLAKY}</p>`);
    await writeReflection(SAMI, meetingId, 'delta', `<p>${SLOW}</p>`);
    await writeReflection(SAMI, meetingId, 'ideas', `<p>${DEMO}</p>`);
    await writeReflection(ROSA, meetingId, 'thanks', `<p>${THANKS}</p>`);
    // Group: two named groups; the idea and the thanks stay on their own.
    await meetingAction(QUINN, meetingId, 'phase/next');
    const shipping = await groupCards(
      ROSA,
      meetingId,
      await cardId(meetingId, TRAIN),
      await cardId(meetingId, PAIRING),
    );
    await renameGroup(ROSA, shipping, SHIPPING);
    const ci = await groupCards(
      SAMI,
      meetingId,
      await cardId(meetingId, FLAKY),
      await cardId(meetingId, SLOW),
    );
    await renameGroup(SAMI, ci, CI);
    await meetingAction(QUINN, meetingId, 'phase/next');
    cspViolations.push(await watchCsp(page));
    await signIn(page, 'quinn');
    quinn = new VotePage(page);
    await quinn.goto(teamId, meetingId);
  });

  test.afterEach(async () => {
    // Every step runs even if one fails, so the team never outlives the test.
    const failures: unknown[] = [];
    for (const context of contexts) await context.close().catch((e: unknown) => failures.push(e));
    await deleteTeam(QUINN, teamId).catch((e: unknown) => failures.push(e));
    if (failures.length > 0) {
      throw new AggregateError(failures, `vote cleanup: ${failures.length} step(s) failed`);
    }
    // The room works within the app's own Content Security Policy.
    expect(cspViolations.flat()).toEqual([]);
  });

  test('every group and loose card can be voted on, with the whole budget left', async ({
    browser,
  }) => {
    const rosa = await open(browser, 'rosa');
    for (const room of [quinn, rosa]) {
      for (const column of ['Plus', 'Delta', 'Ideas', 'Thanks'] as const) {
        await expect(room.column(column)).toBeVisible();
      }
      await expect(room.counter).toHaveText('5 of 5 votes left');
      await expect(room.budgetBar).toContainText(
        'Up to 3 on one group · click a group to read its cards · totals stay hidden until Discuss',
      );
      // Groups: name, card count and a preview of their first card.
      const shipping = room.group(SHIPPING);
      await expect(shipping).toContainText('2 cards');
      await expect(shipping).toContainText(TRAIN);
      await expect(room.column('Delta').getByTestId('vote-group')).toHaveCount(1);
      await expect(room.group(CI)).toContainText('2 cards');
      // Loose cards show as cards, each with a stepper at zero.
      await expect(room.column('Ideas').getByTestId('vote-group')).toHaveCount(1);
      await expect(room.group(DEMO)).toContainText('Ideas');
      await expect(room.group(THANKS)).toContainText('Thanks');
      await expect(room.page.getByTestId('vote-stepper')).toHaveCount(4);
      for (const name of [SHIPPING, CI, DEMO, THANKS]) {
        await expect(room.stepper(name).count).toHaveText('0');
        await expect(room.stepper(name).remove).toBeDisabled();
        await expect(room.stepper(name).add).toBeEnabled();
      }
    }
    // The facilitator alone sees the cast count and the settings.
    await expect(quinn.castCount).toHaveText('0 votes cast so far');
    await expect(quinn.settingsButton).toBeVisible();
    await expect(rosa.castCount).toHaveCount(0);
    await expect(rosa.facilitatorActions).toHaveCount(0);
    await expect(rosa.footer).toContainText("Quinn is facilitating. Press Ready when you're done.");
  });

  test('everyone sees only their own votes; nobody sees totals', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    const sami = await open(browser, 'sami');

    await rosa.addVotes(CI, 2);
    await expect(rosa.stepper(CI).count).toHaveText('2');
    await expect(rosa.stepper(CI).root).toHaveAttribute(
      'aria-label',
      `Your votes on ${CI}: 2 of 3`,
    );
    await expect(rosa.counter).toHaveText('3 of 5 votes left');

    await sami.addVotes(SHIPPING, 1);
    await expect(sami.stepper(SHIPPING).count).toHaveText('1');
    // Neither sees the other's votes, live or after a reload.
    await expect(sami.stepper(CI).count).toHaveText('0');
    await expect(sami.counter).toHaveText('4 of 5 votes left');
    await expect(rosa.stepper(SHIPPING).count).toHaveText('0');
    await sami.page.reload();
    await expect(sami.phase).toHaveText('Vote');
    await expect(sami.stepper(CI).count).toHaveText('0');
    await expect(sami.stepper(SHIPPING).count).toHaveText('1');

    // The facilitator sees one number, and has no votes of their own yet.
    await expect(quinn.castCount).toHaveText('3 votes cast so far');
    await expect(quinn.stepper(CI).count).toHaveText('0');
    await expect(quinn.counter).toHaveText('5 of 5 votes left');

    // No totals in what the server sends: groups carry no votes.
    const samiState = await meetingState(SAMI, meetingId);
    expect(samiState.myVotes).toEqual([{ groupId: await groupOf(meetingId, TRAIN), count: 1 }]);
    expect(samiState.votesCast).toBeNull();
    for (const group of samiState.groups) {
      expect(Object.keys(group).sort()).toEqual(['column', 'id', 'name', 'position']);
    }
    const rosaState = await meetingState(ROSA, meetingId);
    expect(rosaState.myVotes).toEqual([{ groupId: await groupOf(meetingId, FLAKY), count: 2 }]);
    const quinnState = await meetingState(QUINN, meetingId);
    expect(quinnState.myVotes).toEqual([]);
    // One number, nothing per person.
    expect(quinnState.votesCast).toBe(3);
    expect(quinnState).not.toHaveProperty('voteUsage');
  });

  test('− takes a vote back, down to zero', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    const stepper = rosa.stepper(SHIPPING);
    await expect(stepper.remove).toBeDisabled();
    await rosa.addVotes(SHIPPING, 2);
    await expect(stepper.remove).toBeEnabled();
    await expect(rosa.counter).toHaveText('3 of 5 votes left');
    // The facilitator sees both, before they go back.
    await expect(quinn.castCount).toHaveText('2 votes cast so far');

    await rosa.removeVotes(SHIPPING, 1);
    await expect(stepper.count).toHaveText('1');
    await expect(rosa.counter).toHaveText('4 of 5 votes left');
    await rosa.removeVotes(SHIPPING, 1);
    await expect(stepper.count).toHaveText('0');
    await expect(stepper.remove).toBeDisabled();
    await expect(rosa.counter).toHaveText('5 of 5 votes left');
    await expect(quinn.castCount).toHaveText('0 votes cast so far');
  });

  test('a group takes at most 3 of one person’s votes', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    await rosa.addVotes(CI, 3);
    await expect(rosa.stepper(CI).count).toHaveText('3');
    await expect(rosa.stepper(CI).add).toBeDisabled();
    await expect(rosa.counter).toHaveText('2 of 5 votes left');
    // The server refuses a fourth too.
    const refused = await castVote(ROSA, await groupOf(meetingId, FLAKY), 1);
    expect(refused).toEqual({ status: 409, code: 'vote_group_limit' });

    // Other groups still take votes.
    await expect(rosa.stepper(DEMO).add).toBeEnabled();
    await rosa.addVotes(DEMO, 1);
    await expect(rosa.stepper(DEMO).count).toHaveText('1');
    await expect(rosa.counter).toHaveText('1 of 5 votes left');
  });

  test('a spent budget disables every +, until a vote is taken back', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    await rosa.addVotes(CI, 3);
    await rosa.addVotes(SHIPPING, 2);
    await expect(rosa.counter).toHaveText('0 of 5 votes left');
    for (const name of [SHIPPING, CI, DEMO, THANKS]) {
      await expect(rosa.stepper(name).add).toBeDisabled();
    }
    const refused = await castVote(ROSA, await groupOf(meetingId, DEMO), 1);
    expect(refused).toEqual({ status: 409, code: 'vote_budget_exhausted' });

    await rosa.removeVotes(SHIPPING, 1);
    await expect(rosa.counter).toHaveText('1 of 5 votes left');
    await expect(rosa.stepper(SHIPPING).add).toBeEnabled();
    await expect(rosa.stepper(DEMO).add).toBeEnabled();
    await expect(rosa.stepper(THANKS).add).toBeEnabled();
    // Flaky CI is still at its maximum.
    await expect(rosa.stepper(CI).add).toBeDisabled();
  });

  test('votes survive a reload and follow the voter to another tab', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    await rosa.addVotes(DEMO, 2);
    await rosa.page.reload();
    await expect(rosa.phase).toHaveText('Vote');
    await expect(rosa.stepper(DEMO).count).toHaveText('2');
    await expect(rosa.counter).toHaveText('3 of 5 votes left');

    // A second tab of Rosa's follows the first live, and the other way round.
    const tab = new VotePage(await rosa.page.context().newPage());
    cspViolations.push(await watchCsp(tab.page));
    // Live only once its subscription is acknowledged and the reload after it is in.
    const subscribed = channelSubscribed(tab.page, `meeting:${meetingId}`);
    const refreshed = tab.page.waitForResponse(
      (r) => r.request().method() === 'GET' && new URL(r.url()).pathname.endsWith('/state'),
    );
    await tab.goto(teamId, meetingId);
    await Promise.all([subscribed, refreshed]);
    await expect(tab.stepper(DEMO).count).toHaveText('2');
    await rosa.addVotes(THANKS, 1);
    await expect(tab.stepper(THANKS).count).toHaveText('1');
    await expect(tab.counter).toHaveText('2 of 5 votes left');
    await tab.removeVotes(DEMO, 1);
    await expect(rosa.stepper(DEMO).count).toHaveText('1');
    await expect(rosa.counter).toHaveText('3 of 5 votes left');
  });

  test('the facilitator watches the cast count change live', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    const sami = await open(browser, 'sami');
    await expect(quinn.castCount).toHaveText('0 votes cast so far');
    await rosa.addVotes(SHIPPING, 1);
    await expect(quinn.castCount).toHaveText('1 vote cast so far');
    await sami.addVotes(CI, 2);
    await expect(quinn.castCount).toHaveText('3 votes cast so far');
    await rosa.removeVotes(SHIPPING, 1);
    await expect(quinn.castCount).toHaveText('2 votes cast so far');
    // The facilitator's own votes count too.
    await quinn.addVotes(DEMO, 1);
    await expect(quinn.castCount).toHaveText('3 votes cast so far');
    await expect(quinn.counter).toHaveText('4 of 5 votes left');
    // Participants never see it.
    await expect(rosa.castCount).toHaveCount(0);
    await expect(sami.castCount).toHaveCount(0);
  });

  test('a group expands to show its cards, for this viewer only', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    const sami = await open(browser, 'sami');
    await expect(rosa.cardsIn(CI)).toHaveCount(0);
    await rosa.expand(CI);
    await expect(rosa.cardsIn(CI)).toHaveCount(2);
    await expect(rosa.cardsIn(CI).nth(0)).toContainText('Delta');
    await expect(rosa.cardsIn(CI).nth(0)).toContainText(FLAKY);
    await expect(rosa.cardsIn(CI).nth(1)).toContainText(SLOW);
    // One stepper, voting from the open group.
    await expect(rosa.group(CI).getByTestId('vote-stepper')).toHaveCount(1);
    await rosa.addVotes(CI, 1);
    await expect(rosa.stepper(CI).count).toHaveText('1');

    // Expanding is local: Sami's stays collapsed.
    await expect(sami.cardsIn(CI)).toHaveCount(0);
    await rosa.collapse(CI);
    await expect(rosa.cardsIn(CI)).toHaveCount(0);
    await expect(rosa.stepper(CI).count).toHaveText('1');
  });

  test('voting works from the keyboard and keeps the focus', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    const stepper = rosa.stepper(DEMO);
    await stepper.add.focus();
    await rosa.pressVote('Enter');
    await expect(stepper.count).toHaveText('1');
    await expect(stepper.add).toBeFocused();
    await rosa.pressVote('Space');
    await expect(stepper.count).toHaveText('2');
    // The third reaches the maximum: + turns off and the focus moves to −.
    await rosa.pressVote('Enter');
    await expect(stepper.root).toHaveAttribute('aria-label', `Your votes on ${DEMO}: 3 of 3`);
    await expect(stepper.add).toBeDisabled();
    await expect(stepper.remove).toBeFocused();
    await rosa.pressVote('Enter');
    await expect(stepper.count).toHaveText('2');
    // Tab goes from − to + (the count is not a stop).
    await rosa.page.keyboard.press('Tab');
    await expect(stepper.add).toBeFocused();
  });

  test('the facilitator changes the settings; everyone gets them live', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    await rosa.addVotes(CI, 3);
    await expect(rosa.stepper(CI).add).toBeDisabled();

    await quinn.openSettings();
    await expect(quinn.budgetInput).toHaveValue('5');
    await expect(quinn.perGroupInput).toHaveValue('3');
    await expect(quinn.settingsDialog).toContainText('Default 5');
    await expect(quinn.settingsDialog).toContainText('Default 3');
    // Steppers, then Cancel: nothing changes.
    const more = quinn.settingsDialog.getByRole('button', {
      name: 'Increase: Votes per person',
    });
    await more.click();
    await more.click();
    await expect(quinn.budgetInput).toHaveValue('7');
    await quinn.settingsDialog.getByRole('button', { name: 'Cancel' }).click();
    await expect(quinn.settingsDialog).toBeHidden();
    await quinn.openSettings();
    await expect(quinn.budgetInput).toHaveValue('5');
    // Typed values are kept within bounds.
    await quinn.budgetInput.fill('99');
    await quinn.perGroupInput.focus();
    await expect(quinn.budgetInput).toHaveValue('20');
    await quinn.perGroupInput.fill('0');
    await quinn.budgetInput.focus();
    await expect(quinn.perGroupInput).toHaveValue('1');
    await expect(
      quinn.settingsDialog.getByRole('button', {
        name: 'Decrease: Max votes per group',
      }),
    ).toBeDisabled();
    await quinn.settingsDialog.getByRole('button', { name: 'Close' }).click();
    await expect(quinn.settingsDialog).toBeHidden();

    await quinn.saveSettings(7, 4);
    for (const room of [quinn, rosa]) {
      await expect(room.budgetBar).toContainText('Up to 4 on one group');
    }
    await expect(rosa.counter).toHaveText('4 of 7 votes left');
    await expect(quinn.counter).toHaveText('7 of 7 votes left');
    // A fourth vote on the same group now fits.
    await expect(rosa.stepper(CI).add).toBeEnabled();
    await rosa.addVotes(CI, 1);
    await expect(rosa.stepper(CI).root).toHaveAttribute(
      'aria-label',
      `Your votes on ${CI}: 4 of 4`,
    );
    const state = await meetingState(SAMI, meetingId);
    expect(state.voteBudget).toBe(7);
    expect(state.voteMaxPerGroup).toBe(4);
  });

  test('lower limits remove the most recent votes, with a warning first', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    const sami = await open(browser, 'sami');
    // Rosa: three on Flaky CI first, then two on Shipping. Sami: one vote.
    await rosa.addVotes(CI, 3);
    await rosa.addVotes(SHIPPING, 2);
    await sami.addVotes(DEMO, 1);
    await expect(quinn.castCount).toHaveText('6 votes cast so far');

    await quinn.openSettings();
    await expect(quinn.warning).toHaveCount(0);
    await quinn.budgetInput.fill('3');
    await quinn.perGroupInput.focus();
    // One person, never who.
    await expect(quinn.warning).toContainText('1 person has more votes than the new limits allow.');
    await expect(quinn.warning).toContainText(
      'Their most recent votes will be removed to fit the new limits.',
    );
    await expect(quinn.warning).not.toContainText('Rosa');
    await quinn.settingsDialog.getByRole('button', { name: 'Close' }).click();
    await quinn.saveSettings(3, 3);

    // Rosa's two newest votes (Shipping) are gone, live; Sami keeps hers.
    await expect(rosa.stepper(SHIPPING).count).toHaveText('0');
    await expect(rosa.stepper(CI).count).toHaveText('3');
    await expect(rosa.counter).toHaveText('0 of 3 votes left');
    await expect(sami.stepper(DEMO).count).toHaveText('1');
    await expect(sami.counter).toHaveText('2 of 3 votes left');
    await expect(quinn.castCount).toHaveText('4 votes cast so far');

    // A maximum of 1 per group trims Flaky CI to one vote.
    await quinn.openSettings();
    await quinn.perGroupInput.fill('1');
    await quinn.budgetInput.focus();
    await expect(quinn.warning).toContainText('1 person has more votes than the new limits allow.');
    await quinn.settingsDialog.getByRole('button', { name: 'Close' }).click();
    await quinn.saveSettings(3, 1);
    await expect(rosa.stepper(CI).count).toHaveText('1');
    await expect(rosa.counter).toHaveText('2 of 3 votes left');
    await expect(rosa.stepper(CI).add).toBeDisabled();
    await expect(quinn.castCount).toHaveText('2 votes cast so far');

    // Raising the limits gives nothing back.
    await quinn.saveSettings(5, 3);
    await expect(rosa.stepper(CI).count).toHaveText('1');
    await expect(rosa.counter).toHaveText('4 of 5 votes left');
  });

  test('the settings warning follows votes cast while the dialog is open', async () => {
    await quinn.openSettings();
    await quinn.budgetInput.fill('2');
    await quinn.perGroupInput.focus();
    await expect(quinn.warning).toHaveCount(0);

    // Rosa votes over the draft limit while the dialog stays open.
    const ci = await groupOf(meetingId, FLAKY);
    for (let i = 0; i < 3; i++) expect((await castVote(ROSA, ci, 1)).status).toBe(200);
    await expect(quinn.castCount).toHaveText('3 votes cast so far');
    await expect(quinn.warning).toContainText('1 person has more votes than the new limits allow.');
  });

  test('participants press Ready and the facilitator moves on to Discuss', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    const sami = await open(browser, 'sami');
    await rosa.addVotes(CI, 1);
    await expect(quinn.readyCount).toContainText('0/3');
    await rosa.ready.click();
    await expect(rosa.ready).toHaveText("You're ready");
    await sami.ready.click();
    // Quinn's Ready shows at once: wait for the server to take it.
    const [readySaved] = await Promise.all([
      quinn.page.waitForResponse(
        (r) => r.request().method() === 'PUT' && new URL(r.url()).pathname.endsWith('/ready'),
      ),
      quinn.ready.click(),
    ]);
    expect(readySaved.status()).toBe(200);
    await expect(quinn.readyCount).toContainText('3/3');
    await expect(quinn.readyCount).toHaveAttribute('title', 'Everyone is ready');

    await quinn.nextDiscuss.click();
    for (const room of [quinn, rosa, sami]) {
      await expect(room.phase).toHaveText('Discuss');
      await expect(room.page.getByTestId('vote-stepper')).toHaveCount(0);
      await expect(room.counter).toHaveCount(0);
    }
    await expect(quinn.settingsButton).toHaveCount(0);
    // Votes are closed, and kept.
    const refused = await castVote(ROSA, await groupOf(meetingId, FLAKY), 1);
    expect(refused).toEqual({ status: 409, code: 'wrong_phase' });
    const state = await meetingState(ROSA, meetingId);
    expect(state.myVotes).toEqual([{ groupId: await groupOf(meetingId, FLAKY), count: 1 }]);
  });

  test('someone joining during Vote gets the whole budget', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    await rosa.addVotes(SHIPPING, 2);
    await expect(quinn.castCount).toHaveText('2 votes cast so far');

    const tess = await open(browser, 'tess');
    await expect(tess.counter).toHaveText('5 of 5 votes left');
    for (const name of [SHIPPING, CI, DEMO, THANKS]) {
      await expect(tess.stepper(name).count).toHaveText('0');
    }
    await expect(quinn.readyCount).toContainText('0/4');
    await expect(quinn.castCount).toHaveText('2 votes cast so far');
    await tess.addVotes(CI, 1);
    await expect(quinn.castCount).toHaveText('3 votes cast so far');
  });

  test('the cast count and the settings follow the facilitator role', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    await rosa.addVotes(DEMO, 1);
    await quinn.transferTo(USERS.rosa.name);

    await expect(rosa.castCount).toHaveText('1 vote cast so far');
    await expect(rosa.settingsButton).toBeVisible();
    await expect(quinn.castCount).toHaveCount(0);
    await expect(quinn.settingsButton).toHaveCount(0);
    // Rosa's own votes are unchanged.
    await expect(rosa.stepper(DEMO).count).toHaveText('1');
    await rosa.saveSettings(4, 3);
    await expect(quinn.counter).toHaveText('4 of 4 votes left');
  });

  test('an ended meeting takes no more votes', async ({ browser }) => {
    const rosa = await open(browser, 'rosa');
    await rosa.addVotes(SHIPPING, 1);
    await quinn.end();
    await expect(rosa.ended).toBeVisible();
    await expect(rosa.page.getByTestId('vote-stepper')).toHaveCount(0);
    const refused = await castVote(ROSA, await groupOf(meetingId, TRAIN), 1);
    expect(refused).toEqual({ status: 409, code: 'meeting_ended' });
  });

  test('the Vote screen works in the dark theme', async ({ browser }) => {
    const rosa = await open(browser, 'rosa', { colorScheme: 'dark' });
    await rosa.addVotes(CI, 1);
    // Wells, the budget bar and an active stepper come from the dark tokens.
    await expect(rosa.column('Delta')).toHaveCSS('background-color', 'rgb(23, 19, 42)');
    await expect(quinn.column('Delta')).toHaveCSS('background-color', 'rgb(228, 224, 238)');
    await expect(rosa.budgetBar).toHaveCSS('background-color', 'rgb(44, 36, 69)');
    await expect(rosa.stepper(CI).count.locator('..')).toHaveCSS(
      'background-color',
      'rgb(155, 123, 242)',
    );
    await quinn.addVotes(CI, 1);
    await expect(quinn.stepper(CI).count.locator('..')).toHaveCSS(
      'background-color',
      'rgb(115, 72, 214)',
    );
    // The settings dialog and its warning, in the dark.
    const dark = await open(browser, 'quinn', { colorScheme: 'dark' });
    await rosa.addVotes(CI, 2);
    await dark.openSettings();
    await expect(dark.settingsDialog).toHaveCSS('background-color', 'rgb(44, 36, 69)');
    await dark.perGroupInput.fill('1');
    await dark.budgetInput.focus();
    await expect(dark.warning).toHaveCSS('background-color', 'rgb(58, 38, 24)');
  });

  test('the Vote screen works at phone width', async ({ browser }) => {
    const rosa = await open(browser, 'rosa', {
      viewport: { width: 390, height: 844 },
    });
    // The columns stack.
    const plus = (await rosa.column('Plus').boundingBox())!;
    const delta = (await rosa.column('Delta').boundingBox())!;
    expect(delta.y).toBeGreaterThanOrEqual(plus.y + plus.height - 1);
    expect(plus.width).toBeLessThanOrEqual(390);
    // No sideways scrolling.
    expect(
      await rosa.page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
    // Touch targets stay at least 40 px, and voting works.
    const add = rosa.stepper(THANKS).add;
    await add.scrollIntoViewIfNeeded();
    const box = (await add.boundingBox())!;
    expect(box.width).toBeGreaterThanOrEqual(40);
    expect(box.height).toBeGreaterThanOrEqual(40);
    await rosa.addVotes(THANKS, 1);
    await expect(rosa.stepper(THANKS).count).toHaveText('1');
    await expect(rosa.counter).toHaveText('4 of 5 votes left');
  });
});
