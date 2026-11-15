import { expect, test, type Browser, type Page } from '@playwright/test';

import { createTeam, deleteTeam } from '../api';
import { PASSWORD, USERS } from '../fixtures';
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

  test.beforeEach(async ({ page }) => {
    teamId = await createTeam(IVAN, TEAM);
    await signIn(page, 'ivan');
  });

  test.afterEach(async () => {
    await deleteTeam(IVAN, teamId);
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

    // The new meeting appears on Jade's page without a reload.
    const card = jadeMeetings.card('Retro #1');
    await expect(card).toBeVisible();
    await expect(card).toContainText(`Facilitator: ${USERS.ivan.name}`);
    await expect(card).toContainText('Phase: Icebreaker');
    await expect(jade.getByRole('navigation', { name: 'Team' }).getByRole('link', { name: /Meetings/ })).toContainText('1');

    // Jade joins; Ivan sees her in the queue.
    await card.getByRole('link', { name: 'Join Retro #1' }).click();
    const jadeRoom = new MeetingRoomPage(jade);
    await jadeRoom.waitForIcebreaker();
    await expect(jadeRoom.facilitatorActions).toBeHidden();
    await expect(jadeRoom.nextPerson).toBeHidden();
    await expect(ivanRoom.queue.getByRole('listitem')).toHaveCount(2);
    await expect(ivanRoom.answering(1, 2)).toBeVisible();

    // Next person: everyone follows.
    await ivanRoom.nextPerson.click();
    await expect(jadeRoom.answering(2, 2)).toBeVisible();
    await expect(ivanRoom.footer.getByRole('button', { name: 'Next: Reflect' })).toBeVisible();

    // Ivan hands over to Jade, who gets the controls.
    await ivanRoom.transferTo(USERS.jade.name);
    await expect(ivanRoom.facilitatorActions).toBeHidden();
    await expect(jadeRoom.facilitatorActions).toBeVisible();
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
});
