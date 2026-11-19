import { expect, test, type Browser, type Page, type WebSocketRoute } from '@playwright/test';

import {
  createTeam,
  deleteTeam,
  meetingInvitePath,
  removeMember,
  renameMeeting,
  startMeeting,
  userIdOf,
} from '../api';
import { watchCsp } from '../csp';
import { PASSWORD, USERS } from '../fixtures';
import { channelSubscribed } from '../realtime';
import { AppShell } from '../pages/AppShell';
import { MeetingRoomPage } from '../pages/MeetingRoomPage';
import { MeetingsPage } from '../pages/MeetingsPage';
import { SignInPage } from '../pages/SignInPage';
import { TeamSettingsPage } from '../pages/TeamSettingsPage';

const TEAM = 'Meetings E2E';
const IVAN = USERS.ivan.username;
const MEETING_ROOM = /\/teams\/[^/]+\/meetings\/[^/]+$/;

// This spec owns ivan, jade and kai, so it never races the other specs.
test.describe.configure({ mode: 'serial' });

async function signIn(page: Page, user: keyof typeof USERS): Promise<void> {
  const signInPage = new SignInPage(page);
  await signInPage.goto();
  await signInPage.signIn(USERS[user].username, PASSWORD);
  await expect(new AppShell(page).currentUser).toHaveText(USERS[user].name);
}

async function newUser(browser: Browser, user: keyof typeof USERS): Promise<Page> {
  const page = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await signIn(page, user);
  return page;
}

test.describe('meetings', () => {
  let teamId: string;
  let cspViolations: string[];

  test.beforeEach(async ({ page }) => {
    teamId = await createTeam(IVAN, TEAM);
    cspViolations = await watchCsp(page);
    await signIn(page, 'ivan');
  });

  test.afterEach(async () => {
    await deleteTeam(IVAN, teamId);
    // The room works within the app's own Content Security Policy.
    expect(cspViolations).toEqual([]);
  });

  /** Jade joins the team through its join link. */
  async function jadeJoins(page: Page, browser: Browser): Promise<Page> {
    const settings = new TeamSettingsPage(page);
    await settings.goto(teamId);
    const joinLink = await settings.joinLink.inputValue();
    const jade = await newUser(browser, 'jade');
    await jade.goto(new URL(joinLink).pathname);
    await expect(jade.getByRole('heading', { level: 1 })).toHaveText(`You've joined ${TEAM}`);
    return jade;
  }

  test('start, icebreaker, transfer and end, live for everyone', async ({ page, browser }) => {
    const jade = await jadeJoins(page, browser);
    const jadeMeetings = new MeetingsPage(jade);
    await jadeMeetings.goto(teamId);
    await expect(jade.getByText('No meeting is running.')).toBeVisible();

    // Ivan starts a meeting and lands in its room as the facilitator.
    const meetings = new MeetingsPage(page);
    await meetings.goto(teamId);
    await meetings.addMeeting.click();
    await expect(page).toHaveURL(MEETING_ROOM);
    const ivanRoom = new MeetingRoomPage(page);
    await ivanRoom.waitForIcebreaker();
    await expect(ivanRoom.sidebar).toContainText('Retro #1');
    await expect(ivanRoom.sidebar).toContainText(`${USERS.ivan.name} (you)`);
    await expect(ivanRoom.facilitatorActions).toBeVisible();
    await expect(page.getByText("Today's question", { exact: true })).toBeVisible();
    await expect(ivanRoom.greetingPronunciation).toBeVisible();

    // The new meeting appears on Jade's page without a reload.
    const card = jadeMeetings.card('Retro #1');
    await expect(card).toBeVisible();
    await expect(card).toContainText(`Facilitator: ${USERS.ivan.name}`);
    await expect(card).toContainText('Phase: Icebreaker');
    await expect(
      jade.getByRole('navigation', { name: 'Team' }).getByRole('link', { name: /Meetings/ }),
    ).toContainText('1');

    // Jade joins; Ivan sees her in the queue.
    await card.getByRole('link', { name: 'Join Retro #1' }).click();
    const jadeRoom = new MeetingRoomPage(jade);
    await jadeRoom.waitForIcebreaker();
    await expect(jadeRoom.facilitatorActions).toBeHidden();
    await expect(jadeRoom.nextPerson).toBeHidden();
    await expect(ivanRoom.queue.getByRole('listitem')).toHaveCount(2);
    await expect(ivanRoom.answering(1, 2)).toBeVisible();

    // Ivan changes the question; Jade sees the new one, and has no ↻ herself.
    await expect(jadeRoom.changeQuestion).toHaveCount(0);
    await expect(ivanRoom.changeQuestion).toHaveAttribute(
      'title',
      'Change question (facilitator only)',
    );
    const before = (await ivanRoom.question.textContent())!.trim();
    await expect(jadeRoom.question).toHaveText(before);
    await ivanRoom.changeQuestion.click();
    await expect(ivanRoom.question).not.toHaveText(before);
    const after = (await ivanRoom.question.textContent())!.trim();
    await expect(jadeRoom.question).toHaveText(after);

    // Next person: everyone follows.
    await ivanRoom.nextPerson.click();
    await expect(jadeRoom.answering(2, 2)).toBeVisible();
    await expect(ivanRoom.footer.getByRole('button', { name: 'Next: Reflect' })).toBeVisible();

    // Previous person, then back again from the speaking order.
    await ivanRoom.previousPerson.click();
    await expect(jadeRoom.answering(1, 2)).toBeVisible();
    await expect(ivanRoom.previousPerson).toBeDisabled();
    await ivanRoom.nextPerson.click();
    await expect(jadeRoom.answering(2, 2)).toBeVisible();
    await expect(jadeRoom.queue.getByRole('button')).toHaveCount(0);
    await ivanRoom.goBackTo(USERS.ivan.name).click();
    await expect(jadeRoom.answering(1, 2)).toBeVisible();
    await ivanRoom.nextPerson.click();
    await expect(jadeRoom.answering(2, 2)).toBeVisible();

    // Ivan hands over to Jade, who gets the controls.
    await ivanRoom.transferTo(USERS.jade.name);
    await expect(ivanRoom.facilitatorActions).toBeHidden();
    await expect(jadeRoom.facilitatorActions).toBeVisible();
    await expect(jadeRoom.changeQuestion).toBeVisible();
    await expect(ivanRoom.changeQuestion).toHaveCount(0);
    await expect(jade.getByText('You are now the facilitator.')).toBeVisible();
    await expect(page.getByText(`${USERS.jade.name} is now the facilitator.`)).toBeVisible();

    // Jade moves on to Reflect, then ends the meeting.
    await jadeRoom.footer.getByRole('button', { name: 'Next: Reflect' }).click();
    await expect(ivanRoom.phase).toHaveText('Reflect');
    await jadeRoom.end();
    await expect(jadeRoom.ended).toBeVisible();
    await expect(ivanRoom.ended).toBeVisible();

    // It is listed as ended.
    await meetings.goto(teamId);
    await expect(meetings.ended.getByRole('link', { name: /Retro #1/ })).toBeVisible();
    await expect(page.getByText('No meeting is running.')).toBeVisible();
    await jade.context().close();
  });

  test('the meeting invite link brings a stranger into the team and the room', async ({
    page,
    browser,
    context,
  }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const meetings = new MeetingsPage(page);
    await meetings.goto(teamId);
    await meetings.addMeeting.click();
    const room = new MeetingRoomPage(page);
    await room.waitForIcebreaker();

    await page.getByRole('button', { name: 'Invite: copy meeting invite link' }).click();
    await expect(page.getByText('Meeting invite link copied.')).toBeAttached();
    const link = await page.evaluate(() => navigator.clipboard.readText());
    expect(link).toMatch(/\/m\/[\w-]+$/);

    const kai = await newUser(browser, 'kai');
    await kai.goto(new URL(link).pathname);
    await expect(kai.getByRole('heading', { level: 1 })).toHaveText('Joining Retro #1…');
    await expect(kai.getByText(`You've been added to ${TEAM} as a member.`)).toBeVisible();
    await expect(kai).toHaveURL(MEETING_ROOM);
    await new MeetingRoomPage(kai).waitForIcebreaker();
    await expect(room.queue.getByRole('listitem')).toHaveCount(2);

    // Once ended, the link stops working.
    await room.end();
    await expect(room.ended).toBeVisible();
    await kai.goto(new URL(link).pathname);
    await expect(kai.getByRole('heading', { level: 1 })).toHaveText('This meeting has ended');
    await kai.context().close();
  });

  test('a removed member is cut off at once and cannot come back with the link', async ({
    page,
    browser,
  }) => {
    const meetings = new MeetingsPage(page);
    await meetings.goto(teamId);
    await meetings.addMeeting.click();
    await expect(page).toHaveURL(MEETING_ROOM);
    const room = new MeetingRoomPage(page);
    await room.waitForIcebreaker();
    const meetingId = new URL(page.url()).pathname.split('/').pop()!;
    const invite = await meetingInvitePath(IVAN, meetingId);

    // Kai comes in through the meeting invite link.
    const kai = await newUser(browser, 'kai');
    await kai.goto(invite);
    const kaiRoom = new MeetingRoomPage(kai);
    await kaiRoom.waitForIcebreaker();
    await expect(room.queue.getByRole('listitem')).toHaveCount(2);

    // Ivan removes Kai: Kai's room stops live, without a reload.
    await removeMember(IVAN, teamId, await userIdOf('kai'));
    await expect(
      kai.getByText('This team no longer exists or you are no longer a member.'),
    ).toBeVisible();
    await expect(kaiRoom.footer).toBeHidden();

    // Later changes reach the team only.
    await renameMeeting(IVAN, meetingId, 'After Kai');
    await expect(room.sidebar).toContainText('After Kai');
    await expect(kai.getByText('After Kai')).toHaveCount(0);

    // Kai is not offered the facilitator role.
    await room.openTransfer();
    await expect(room.transferOptions()).toHaveCount(0);
    await page.keyboard.press('Escape');
    await expect(room.transferDialog).toBeHidden();

    // The link Kai knew no longer works, and the team stays closed to Kai.
    // (The page shows every refused meeting link as ended.)
    await kai.goto(invite);
    await expect(kai.getByRole('heading', { level: 1 })).toHaveText('This meeting has ended');
    await expect(kai).not.toHaveURL(MEETING_ROOM);
    await kai.goto(`/teams/${teamId}/meetings`);
    await expect(kai).not.toHaveURL(new RegExp(teamId));
    await kai.context().close();
  });

  test('the Meetings page follows people entering and leaving a room without reloading', async ({
    page,
    browser,
  }) => {
    const jade = await jadeJoins(page, browser);
    const meetingId = await startMeeting(IVAN, teamId);
    const jadeMeetings = new MeetingsPage(jade);
    const isList = (url: string) =>
      /^\/api\/v1\/teams\/[^/]+\/meetings$/.test(new URL(url).pathname);
    // Once the team channel is live the page re-reads the list, once: what it
    // loaded before may have missed events. Let that happen before counting.
    // (The first load's request went out at mount, before the socket opened.)
    const reread = channelSubscribed(jade, `team:${teamId}`)
      .then(() => jade.waitForRequest((r) => r.method() === 'GET' && isList(r.url())))
      .then((request) => request.response());
    await jadeMeetings.goto(teamId);
    const card = jadeMeetings.card('Retro #1');
    await expect(card).toBeVisible();
    await expect(jadeMeetings.onlineDots(card)).toHaveCount(0);
    await reread;

    let reloads = 0;
    jade.on('request', (request) => {
      if (request.method() === 'GET' && isList(request.url())) reloads++;
    });

    // Ivan enters the room, leaves it, and comes back.
    const room = new MeetingRoomPage(page);
    await page.goto(`/teams/${teamId}/meetings/${meetingId}`);
    await room.waitForIcebreaker();
    await expect(jadeMeetings.onlineDots(card)).toHaveCount(1);
    await new MeetingsPage(page).goto(teamId);
    await expect(jadeMeetings.onlineDots(card)).toHaveCount(0);
    await page.goto(`/teams/${teamId}/meetings/${meetingId}`);
    await room.waitForIcebreaker();
    await expect(jadeMeetings.onlineDots(card)).toHaveCount(1);

    expect(reloads).toBe(0);
    await jade.context().close();
  });

  test('the room shows the reconnecting banner only once the connection is lost', async ({
    page,
  }) => {
    const meetingId = await startMeeting(IVAN, teamId);
    let socket: WebSocketRoute | undefined;
    await page.routeWebSocket(/\/api\/v1\/ws\?/, (ws) => {
      socket = ws;
      ws.connectToServer();
    });
    // Hold the socket's ticket: the room loads while the socket still connects.
    let release!: () => void;
    const released = new Promise<void>((resolve) => (release = resolve));
    await page.route('**/api/v1/ws-ticket', async (route) => {
      await released;
      await route.continue();
    });

    const room = new MeetingRoomPage(page);
    await page.goto(`/teams/${teamId}/meetings/${meetingId}`);
    await room.waitForIcebreaker();
    expect(socket).toBeUndefined();
    await expect(room.reconnecting).toBeHidden();

    release();
    await expect.poll(() => socket !== undefined).toBe(true);
    await expect(room.reconnecting).toBeHidden();

    // The connection drops: the banner shows until the socket is back.
    const first = socket!;
    await first.close();
    await expect(room.reconnecting).toBeVisible();
    await expect.poll(() => socket !== first, { timeout: 15_000 }).toBe(true);
    await expect(room.reconnecting).toBeHidden({ timeout: 15_000 });
  });
});
