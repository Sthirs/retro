import path from 'node:path';

import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';

import {
  castVote,
  createTask,
  createTeam,
  deleteTask,
  deleteTeam,
  editTask,
  groupCards,
  joinMeeting,
  joinTeam,
  meetingAction,
  meetingState,
  moveTask,
  removeMember,
  renameGroup,
  renameMeeting,
  seedTask,
  startMeeting,
  summaryStatus,
  uploadImage,
  userIdOf,
  writeReflection,
} from '../api';
import { watchCsp } from '../csp';
import { PASSWORD, USERS } from '../fixtures';
import { channelSubscribed } from '../realtime';
import { AppShell } from '../pages/AppShell';
import { DiscussPage } from '../pages/DiscussPage';
import { MeetingsPage } from '../pages/MeetingsPage';
import { SignInPage } from '../pages/SignInPage';
import { SummaryPage } from '../pages/SummaryPage';
import { TasksPage } from '../pages/TasksPage';

const TEAM = 'Summary E2E';
const YARA = USERS.yara.username;
const ZENO = USERS.zeno.username;
const ABEL = USERS.abel.username;
const BEA = USERS.bea.username;
const NOAH = USERS.noah.username;
const PNG = path.join(__dirname, '..', 'assets', 'avatar.png');

// The reflections of a meeting run to Discuss (see `toDiscuss`).
const FLAKY = 'Flaky e2e tests blocked';
const SLOW = 'CI takes 25 minutes';
const GUIDE = 'Read the CI guide first';
const TRAIN = 'Release train shipped on time';
const SMALL = 'Smaller PRs made reviews faster';
/** Never grouped: a group of its own, named by its text. */
const THANKS = 'Thanks Abel for pairing';
const CI = 'Flaky CI';
const SHIPPING = 'Shipping';

// The tasks written in Discuss.
const QUARANTINE = 'Quarantine flaky tests';
const SPLIT = 'Split the CI pipeline';
const DOC_DAY = 'Hold a doc-day';

const SUMMARY_URL = /\/teams\/[^/]+\/meetings\/[^/]+\/summary$/;
const ROOM_URL = /\/teams\/[^/]+\/meetings\/[^/]+$/;

// This spec owns yara, zeno, abel, bea and noah, so it never races the other specs.
test.describe.configure({ mode: 'serial' });

async function signIn(page: Page, user: keyof typeof USERS): Promise<void> {
  const signInPage = new SignInPage(page);
  await signInPage.goto();
  await signInPage.signIn(USERS[user].username, PASSWORD);
  await expect(new AppShell(page).currentUser).toHaveText(USERS[user].name);
}

async function cardId(meetingId: string, text: string): Promise<string> {
  const state = await meetingState(YARA, meetingId);
  return state.reflections.find((r) => r.content.includes(text))!.id;
}

/** "10 Nov 2026", the way the app writes dates. */
function formatDate(iso: string): string {
  const parts = new Intl.DateTimeFormat('en-GB', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  }).formatToParts(new Date(iso));
  const get = (type: string): string => parts.find((p) => p.type === type)?.value ?? '';
  return `${get('day')} ${get('month').slice(0, 3)} ${get('year')}`;
}

test.describe('meeting summary', () => {
  let teamId: string;
  let meetingId: string;
  let cspViolations: string[][];
  let contexts: BrowserContext[];
  /** Yara, the facilitator (team admin), in the test's own page. */
  let yara: Page;

  interface OpenOptions {
    colorScheme?: 'light' | 'dark';
    viewport?: { width: number; height: number };
  }

  /** Someone signed in, in their own browser. */
  async function open(
    browser: Browser,
    user: keyof typeof USERS,
    options: OpenOptions = {},
  ): Promise<Page> {
    const context = await browser.newContext({
      ignoreHTTPSErrors: true,
      colorScheme: options.colorScheme ?? 'light',
      ...(options.viewport ? { viewport: options.viewport } : {}),
    });
    contexts.push(context);
    const page = await context.newPage();
    cspViolations.push(await watchCsp(page));
    await signIn(page, user);
    return page;
  }

  /** Opens the Discuss room and waits until it is live: events reach it from now on. */
  async function liveRoom(page: Page): Promise<DiscussPage> {
    const room = new DiscussPage(page);
    const subscribed = channelSubscribed(page, `meeting:${meetingId}`);
    const refreshed = page.waitForResponse(
      (r) => r.request().method() === 'GET' && new URL(r.url()).pathname.endsWith('/state'),
    );
    await room.goto(teamId, meetingId);
    await Promise.all([subscribed, refreshed]);
    return room;
  }

  /**
   * Runs the meeting to Discuss through the API. Groups: "Flaky CI" (three
   * cards, 4 votes), "Shipping" (two cards, one with an image, 2 votes) and
   * the thanks card alone (2 votes): Shipping is bigger, so it ranks first of
   * the two. Then three tasks on Flaky CI, the first mentioning Zeno.
   */
  async function toDiscuss(): Promise<void> {
    await meetingAction(YARA, meetingId, 'phase/next');
    await writeReflection(ZENO, meetingId, 'delta', `<p>${FLAKY} <strong>merges</strong></p>`);
    await writeReflection(ABEL, meetingId, 'delta', `<p>${SLOW}</p>`);
    await writeReflection(
      ABEL,
      meetingId,
      'ideas',
      `<p>${GUIDE}: <a href="https://example.com/ci-guide">the guide</a></p>`,
    );
    const image = await uploadImage(YARA, teamId, PNG, 'image/png');
    await writeReflection(
      YARA,
      meetingId,
      'plus',
      `<p>${TRAIN}</p><p><img data-attachment-id="${image}" alt="Train"></p>`,
    );
    await writeReflection(ABEL, meetingId, 'plus', `<p>${SMALL}</p>`);
    await writeReflection(ZENO, meetingId, 'thanks', `<p>${THANKS}</p>`);

    await meetingAction(YARA, meetingId, 'phase/next');
    const ci = await groupCards(
      ZENO,
      meetingId,
      await cardId(meetingId, FLAKY),
      await cardId(meetingId, SLOW),
    );
    await groupCards(
      ZENO,
      meetingId,
      await cardId(meetingId, FLAKY),
      await cardId(meetingId, GUIDE),
    );
    await renameGroup(ZENO, ci, CI);
    const shipping = await groupCards(
      ABEL,
      meetingId,
      await cardId(meetingId, TRAIN),
      await cardId(meetingId, SMALL),
    );
    await renameGroup(ABEL, shipping, SHIPPING);

    await meetingAction(YARA, meetingId, 'phase/next');
    const thanks = (await meetingState(YARA, meetingId)).reflections.find((r) =>
      r.content.includes(THANKS),
    )!.groupId!;
    for (const [user, group, times] of [
      [ZENO, ci, 3],
      [ZENO, thanks, 2],
      [ABEL, ci, 1],
      [ABEL, shipping, 2],
    ] as const) {
      for (let i = 0; i < times; i++) {
        expect(await castVote(user, group, 1)).toEqual({
          status: 200,
          code: null,
        });
      }
    }

    await meetingAction(YARA, meetingId, 'phase/next');
    const state = await meetingState(YARA, meetingId);
    expect(state.discussGroupId).toBe(ci);
    const zeno = await userIdOf(ZENO);
    await createTask(
      YARA,
      meetingId,
      ci,
      `<p>${QUARANTINE} <span data-type="mention" data-id="${zeno}" data-label="${USERS.zeno.name}">@${USERS.zeno.name}</span></p>`,
    );
    await createTask(ABEL, meetingId, ci, `<p>${SPLIT}</p>`);
    await createTask(ZENO, meetingId, ci, `<p>${DOC_DAY}</p>`);
  }

  /** Runs the meeting to Discuss and ends it (through the API). */
  async function ended(): Promise<void> {
    await toDiscuss();
    await meetingAction(YARA, meetingId, 'end');
  }

  async function taskId(text: string): Promise<string> {
    const state = await meetingState(YARA, meetingId);
    return state.tasks.find((t) => t.contentHtml.includes(text))!.id;
  }

  test.beforeEach(async ({ page }) => {
    contexts = [];
    cspViolations = [];
    // Yara facilitates; Zeno and Abel take part; Bea is a member who never joins.
    teamId = await createTeam(YARA, TEAM);
    for (const user of [ZENO, ABEL, BEA]) await joinTeam(YARA, teamId, user);
    meetingId = await startMeeting(YARA, teamId);
    for (const user of [ZENO, ABEL]) await joinMeeting(user, teamId, meetingId);
    cspViolations.push(await watchCsp(page));
    await signIn(page, 'yara');
    yara = page;
  });

  test.afterEach(async () => {
    // Every step runs even if one fails, so the team never outlives the test.
    const failures: unknown[] = [];
    for (const context of contexts) await context.close().catch((e: unknown) => failures.push(e));
    await deleteTeam(YARA, teamId).catch((e: unknown) => failures.push(e));
    if (failures.length > 0) {
      throw new AggregateError(failures, `summary cleanup: ${failures.length} step(s) failed`);
    }
    // The summary works within the app's own Content Security Policy.
    expect(cspViolations.flat()).toEqual([]);
  });

  test('ending the meeting takes everyone in the room to its summary', async ({ browser }) => {
    await toDiscuss();
    const facilitator = await liveRoom(yara);
    const zeno = await liveRoom(await open(browser, 'zeno'));
    await expect(zeno.groupTitle).toHaveText(CI);

    // The facilitator walks to the last group and ends the meeting there.
    await facilitator.nextGroup.click();
    await facilitator.nextGroup.click();
    await expect(zeno.groupTitle).toHaveText(THANKS);
    await facilitator.endMeeting.click();
    await expect(facilitator.endDialog).toBeVisible();
    await facilitator.endDialog.getByRole('button', { name: 'End meeting' }).click();

    for (const page of [yara, zeno.page]) {
      await expect(page).toHaveURL(SUMMARY_URL);
      const summary = new SummaryPage(page);
      await expect(summary.eyebrow).toBeVisible();
      await expect(summary.heading).toHaveText('Retro #1');
      await expect(summary.endedAt).toHaveText(/Ended at \d\d:\d\d$/);
      await expect(summary.groupTitles()).toHaveText([CI, SHIPPING, THANKS]);
      // The summary sits in the team pages, under Meetings.
      await expect(
        page.getByRole('navigation', { name: 'Team' }).getByRole('link', { name: /Meetings/ }),
      ).toHaveAttribute('aria-current', 'page');
    }
    // The ended meeting is listed, and links to the summary.
    const meetings = new MeetingsPage(yara);
    await meetings.goto(teamId);
    await expect(meetings.ended.getByRole('link', { name: /Retro #1/ })).toHaveAttribute(
      'href',
      SummaryPage.path(teamId, meetingId),
    );
  });

  test('the facts: date, duration, facilitator and votes cast', async () => {
    await toDiscuss();
    // Zeno facilitates by the end: the summary names who ended it.
    await meetingAction(YARA, meetingId, 'facilitator', {
      userId: await userIdOf(ZENO),
    });
    await meetingAction(ZENO, meetingId, 'end');

    const summary = new SummaryPage(yara);
    await summary.goto(teamId, meetingId);
    await expect(summary.fact('Facilitator')).toHaveText(USERS.zeno.name);
    await expect(summary.fact('Votes cast')).toHaveText('8');
    await expect(summary.fact('Duration')).toHaveText(/^\d+ min$/);
    await expect(summary.fact('Date')).toHaveText(formatDate(new Date().toISOString()));
  });

  test('groups ranked by votes, with anonymous cards in rich text', async () => {
    await ended();
    const summary = new SummaryPage(yara);
    await summary.goto(teamId, meetingId);

    // Most votes first; at 2 votes each, the bigger group ranks first.
    await expect(summary.groupTitles()).toHaveText([CI, SHIPPING, THANKS]);
    const ci = summary.group(CI);
    const shipping = summary.group(SHIPPING);
    const thanks = summary.group(THANKS);
    await expect(ci).toContainText('#1');
    await expect(ci).toContainText('4 votes');
    await expect(shipping).toContainText('#2');
    await expect(shipping).toContainText('2 votes');
    await expect(thanks).toContainText('#3');
    await expect(thanks).toContainText('2 votes');

    // Each card keeps its column label, by column then place.
    await expect(summary.cards(ci)).toHaveCount(3);
    await expect(summary.cards(ci).locator('.card__column')).toHaveText([
      'Delta',
      'Delta',
      'Ideas',
    ]);
    await expect(summary.cards(thanks).locator('.card__column')).toHaveText(['Thanks']);
    await expect(summary.cards(shipping).locator('.card__column')).toHaveText(['Plus', 'Plus']);

    // Rich text: bold, links and images.
    await expect(ci.locator('strong')).toHaveText('merges');
    await expect(ci.getByRole('link', { name: 'the guide' })).toHaveAttribute(
      'href',
      'https://example.com/ci-guide',
    );
    const image = shipping.locator('img[data-attachment-id]');
    await expect(image).toHaveAttribute('src', /^blob:/);
    await expect
      .poll(() => image.evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth))
      .toBeGreaterThan(0);

    // Anonymous: nobody's name next to the cards.
    for (const user of [USERS.yara, USERS.zeno, USERS.abel]) {
      await expect(summary.groupsSection).not.toContainText(user.name);
    }
  });

  test('tasks show their current board status, mentions and edits', async () => {
    await ended();
    const summary = new SummaryPage(yara);
    await summary.goto(teamId, meetingId);
    await expect(summary.tasks).toHaveCount(3);
    await expect(summary.tasksSection).toContainText(
      "Showing each task's current status on the board.",
    );
    for (const text of [QUARANTINE, SPLIT, DOC_DAY]) {
      await expect(summary.task(text)).toContainText('Active');
    }
    await expect(summary.mentions(summary.task(QUARANTINE))).toHaveText([`@${USERS.zeno.name}`]);
    await expect(summary.mentions(summary.task(SPLIT))).toHaveCount(0);

    // The board moves on after the meeting: the summary follows it.
    const board = new TasksPage(yara);
    await board.goto(teamId);
    await expect(board.cards('Active')).toHaveCount(3);
    await moveTask(ABEL, await taskId(QUARANTINE), 'done');
    await moveTask(ZENO, await taskId(SPLIT), 'stuck');
    await editTask(BEA, await taskId(SPLIT), `<p>${SPLIT} in two stages</p>`);
    await deleteTask(ZENO, await taskId(DOC_DAY));

    await summary.goto(teamId, meetingId);
    await expect(summary.tasks).toHaveCount(2);
    await expect(summary.task(QUARANTINE)).toContainText('Done');
    await expect(summary.task(SPLIT)).toContainText(`${SPLIT} in two stages`);
    await expect(summary.task(SPLIT)).toContainText('Stuck');
    await expect(summary.task(DOC_DAY)).toHaveCount(0);

    await moveTask(ABEL, await taskId(SPLIT), 'future');
    await yara.reload();
    await expect(summary.task(SPLIT)).toContainText('Future');
  });

  test('the Tasks board link opens the board filtered to this meeting', async () => {
    await ended();
    // A task of another meeting of the team stays out of the filter.
    await seedTask(YARA, teamId, {
      status: 'active',
      content: '<p>Another meeting task</p>',
    });

    const summary = new SummaryPage(yara);
    await summary.goto(teamId, meetingId);
    await summary.boardLink.click();
    await expect(yara).toHaveURL(new RegExp(`/teams/${teamId}/tasks\\?meeting=${meetingId}$`));
    const board = new TasksPage(yara);
    await expect(board.heading).toBeVisible();
    await expect(board.cards('Active')).toHaveCount(3);
    await expect(board.card(QUARANTINE)).toBeVisible();
    await expect(board.card('Another meeting task')).toHaveCount(0);
  });

  test('participants, including someone who has left the team since', async () => {
    await ended();
    await removeMember(YARA, teamId, await userIdOf(ABEL));

    const summary = new SummaryPage(yara);
    await summary.goto(teamId, meetingId);
    await expect(summary.participantsSection.getByRole('heading')).toHaveText('Participants · 3');
    await expect(summary.participantNames).toHaveText([
      USERS.yara.name,
      USERS.zeno.name,
      USERS.abel.name,
    ]);
    // Bea never joined: a member, not a participant.
    await expect(summary.participantsSection).not.toContainText(USERS.bea.name);
    // Abel is out of the team, and so out of its summaries.
    expect(await summaryStatus(ABEL, meetingId)).toEqual({
      status: 404,
      code: 'meeting_not_found',
    });
  });

  test('from the Meetings page to the summary and back', async () => {
    await ended();
    const meetings = new MeetingsPage(yara);
    await meetings.goto(teamId);
    await meetings.ended.getByRole('link', { name: /Retro #1/ }).click();
    await expect(yara).toHaveURL(SUMMARY_URL);

    const summary = new SummaryPage(yara);
    await expect(summary.heading).toHaveText('Retro #1');
    // A reload keeps it.
    await yara.reload();
    await expect(summary.heading).toHaveText('Retro #1');
    await expect(summary.groups).toHaveCount(3);

    await summary.backLink.click();
    await expect(meetings.heading).toBeVisible();
    await expect(yara).toHaveURL(new RegExp(`/teams/${teamId}/meetings$`));
  });

  test("the room of an ended meeting leads to its summary, and a running one's summary to its room", async () => {
    // Running: the summary address opens the room.
    await yara.goto(SummaryPage.path(teamId, meetingId));
    await expect(yara).toHaveURL(ROOM_URL);
    await expect(yara.getByRole('heading', { level: 1 })).toHaveText('Icebreaker');
    expect(await summaryStatus(BEA, meetingId)).toEqual({
      status: 409,
      code: 'meeting_active',
    });

    await meetingAction(YARA, meetingId, 'end');
    // Ended: the room address opens the summary.
    await yara.goto(`/teams/${teamId}/meetings/${meetingId}`);
    await expect(yara).toHaveURL(SUMMARY_URL);
    await expect(new SummaryPage(yara).eyebrow).toBeVisible();
  });

  test('any member can read it, nobody else', async ({ browser }) => {
    await ended();
    // Bea never joined the meeting, but she is in the team.
    const bea = new SummaryPage(await open(browser, 'bea'));
    await bea.goto(teamId, meetingId);
    await expect(bea.groupTitles()).toHaveText([CI, SHIPPING, THANKS]);
    await expect(bea.tasks).toHaveCount(3);

    // Noah is not in the team: the address leads elsewhere.
    const noah = await open(browser, 'noah');
    await noah.goto(SummaryPage.path(teamId, meetingId));
    await expect(noah).not.toHaveURL(new RegExp(teamId));
    await expect(noah.getByText(FLAKY)).toHaveCount(0);
    expect(await summaryStatus(NOAH, meetingId)).toEqual({
      status: 404,
      code: 'meeting_not_found',
    });

    // An unknown meeting of the team.
    const unknown = new SummaryPage(yara);
    await yara.goto(SummaryPage.path(teamId, '0192f000-0000-7000-8000-000000000000'));
    await expect(unknown.notFound).toBeVisible();
    await expect(yara.getByText('This meeting no longer exists.')).toBeVisible();
    await yara.getByRole('link', { name: 'Back to meetings' }).click();
    await expect(new MeetingsPage(yara).heading).toBeVisible();
  });

  test('a meeting ended during Reflect keeps its reflections hidden', async () => {
    await meetingAction(YARA, meetingId, 'phase/next');
    await writeReflection(ZENO, meetingId, 'delta', '<p>Secret worry</p>');
    await meetingAction(YARA, meetingId, 'end');

    const summary = new SummaryPage(yara);
    await summary.goto(teamId, meetingId);
    await expect(summary.groups).toHaveCount(0);
    await expect(summary.groupsSection).toContainText(
      'No reflections were revealed in this meeting.',
    );
    await expect(summary.tasksSection).toContainText('No tasks were created in this meeting.');
    await expect(summary.fact('Votes cast')).toHaveText('0');
    await expect(summary.participants).toHaveCount(3);
    await expect(yara.getByText('Secret worry')).toHaveCount(0);
  });

  test('a meeting ended during Group shows loose cards as groups of their own', async () => {
    await meetingAction(YARA, meetingId, 'phase/next');
    await writeReflection(ZENO, meetingId, 'ideas', '<p>Try mob programming</p>');
    await writeReflection(ABEL, meetingId, 'delta', '<p>Standups run long</p>');
    await writeReflection(YARA, meetingId, 'plus', '<p>Standups started on time</p>');
    await meetingAction(YARA, meetingId, 'phase/next');
    const twins = await groupCards(
      ABEL,
      meetingId,
      await cardId(meetingId, 'Standups run long'),
      await cardId(meetingId, 'Standups started on time'),
    );
    await renameGroup(ABEL, twins, 'Standups');
    await meetingAction(YARA, meetingId, 'end');

    const summary = new SummaryPage(yara);
    await summary.goto(teamId, meetingId);
    await expect(summary.groupTitles()).toHaveText(['Standups', 'Try mob programming']);
    await expect(summary.groups.filter({ hasText: '0 votes' })).toHaveCount(2);
    await expect(summary.cards(summary.group('Standups')).locator('.card__column')).toHaveText([
      'Plus',
      'Delta',
    ]);
    await expect(summary.fact('Votes cast')).toHaveText('0');
    await expect(summary.tasks).toHaveCount(0);
  });

  test('a renamed meeting shows its new title', async () => {
    await renameMeeting(YARA, meetingId, 'Sprint 42 retro');
    await meetingAction(YARA, meetingId, 'end');
    const summary = new SummaryPage(yara);
    await summary.goto(teamId, meetingId);
    await expect(summary.heading).toHaveText('Sprint 42 retro');
    await new MeetingsPage(yara).goto(teamId);
    await expect(
      new MeetingsPage(yara).ended.getByRole('link', {
        name: /Sprint 42 retro/,
      }),
    ).toBeVisible();
  });

  test('read-only: nothing on the summary changes the meeting', async () => {
    await ended();
    const summary = new SummaryPage(yara);
    await summary.goto(teamId, meetingId);
    await expect(summary.groups).toHaveCount(3);
    const main = yara.getByRole('main');
    await expect(main.getByRole('button')).toHaveCount(0);
    await expect(main.getByRole('textbox')).toHaveCount(0);
    await expect(main.getByRole('checkbox')).toHaveCount(0);
    // Only links: back to Meetings, the Tasks board, and the links in the cards.
    await expect(main.getByRole('link', { name: 'Meetings', exact: true })).toBeVisible();
    await expect(summary.boardLink).toBeVisible();
  });

  test('works in the dark theme and on a narrow screen', async ({ browser }) => {
    await ended();
    const light = new SummaryPage(await open(browser, 'zeno'));
    const dark = new SummaryPage(await open(browser, 'abel', { colorScheme: 'dark' }));
    for (const summary of [light, dark]) await summary.goto(teamId, meetingId);
    const background = (summary: SummaryPage) =>
      summary.group(CI).evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(await background(light)).toBe('rgb(255, 255, 255)');
    expect(await background(dark)).not.toBe(await background(light));

    const narrow = new SummaryPage(
      await open(browser, 'bea', { viewport: { width: 390, height: 844 } }),
    );
    await narrow.goto(teamId, meetingId);
    await expect(narrow.groupTitles()).toHaveText([CI, SHIPPING, THANKS]);
    await expect(narrow.tasksSection).toBeVisible();
    await expect(narrow.participantsSection).toBeVisible();
    await expect
      .poll(() =>
        narrow.page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        ),
      )
      .toBe(0);
  });
});
