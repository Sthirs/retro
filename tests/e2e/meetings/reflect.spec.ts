import path from 'node:path';

import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';

import {
  createTeam,
  deleteTeam,
  joinTeam,
  meetingAction,
  meetingState,
  setRole,
  startMeeting,
  userIdOf,
  writeReflection,
} from '../api';
import { watchCsp } from '../csp';
import { PASSWORD, USERS } from '../fixtures';
import { AppShell } from '../pages/AppShell';
import { ReflectPage, secondsLeft } from '../pages/ReflectPage';
import { SignInPage } from '../pages/SignInPage';

const TEAM = 'Reflect E2E';
const GIF = path.join(__dirname, '..', 'assets', 'anim.gif');
const LENA = USERS.lena.username;
const MILO = USERS.milo.username;
const NORA = USERS.nora.username;

// This spec owns lena, milo and nora, so it never races the other specs.
test.describe.configure({ mode: 'serial' });

async function signIn(page: Page, user: keyof typeof USERS): Promise<void> {
  const signInPage = new SignInPage(page);
  await signInPage.goto();
  await signInPage.signIn(USERS[user].username, PASSWORD);
  await expect(new AppShell(page).currentUser).toHaveText(USERS[user].name);
}

test.describe('reflect', () => {
  let teamId: string;
  let meetingId: string;
  let cspViolations: string[][];
  let contexts: BrowserContext[];
  /** Lena, the facilitator (team admin). */
  let lena: ReflectPage;

  /** Another participant in their own browser, in the meeting room. */
  async function open(browser: Browser, user: keyof typeof USERS): Promise<ReflectPage> {
    const context = await browser.newContext({ ignoreHTTPSErrors: true });
    contexts.push(context);
    const page = await context.newPage();
    cspViolations.push(await watchCsp(page));
    await signIn(page, user);
    const room = new ReflectPage(page);
    await room.goto(teamId, meetingId);
    return room;
  }

  test.beforeEach(async ({ page }) => {
    contexts = [];
    cspViolations = [];
    // A team of three with a meeting in Reflect: Lena facilitates, Milo joined.
    teamId = await createTeam(LENA, TEAM);
    await joinTeam(LENA, teamId, MILO);
    await joinTeam(LENA, teamId, NORA);
    meetingId = await startMeeting(LENA, teamId);
    await meetingAction(MILO, meetingId, 'join');
    await meetingAction(LENA, meetingId, 'phase/next');
    cspViolations.push(await watchCsp(page));
    await signIn(page, 'lena');
    lena = new ReflectPage(page);
    await lena.goto(teamId, meetingId);
  });

  test.afterEach(async () => {
    for (const context of contexts) await context.close();
    await deleteTeam(LENA, teamId);
    // The room works within the app's own Content Security Policy.
    expect(cspViolations.flat()).toEqual([]);
  });

  test('participants write in every column; the others see only counts', async ({ browser }) => {
    const milo = await open(browser, 'milo');
    await expect(milo.phase).toHaveText('Reflect');
    await expect(milo.page.getByText("Others only see how many you've written.")).toBeVisible();

    // Send with the button, and with Ctrl + Enter.
    await milo.write('Plus', 'Release train shipped on time');
    await milo.startWriting('Delta');
    await milo.page.keyboard.type('Flaky e2e tests blocked merges');
    await milo.page.keyboard.press('Control+Enter');
    await expect(milo.mine('Delta')).toContainText('Flaky e2e tests blocked merges');
    await milo.write('Ideas', 'Rotate the on-call buddy');
    await milo.write('Thanks', 'Thanks Lena for the release notes');
    await milo.write('Thanks', 'Thanks Nora for pairing');

    for (const name of ['Plus', 'Delta', 'Ideas'] as const) {
      await expect(milo.column(name)).toContainText('1 mine');
      await expect(milo.mine(name)).toContainText('Sent · yours');
    }
    await expect(milo.column('Thanks')).toContainText('2 mine');

    // Lena sees how many, never what.
    await expect(lena.team('Plus')).toHaveText('1 from the team');
    await expect(lena.team('Delta')).toHaveText('1 from the team');
    await expect(lena.team('Ideas')).toHaveText('1 from the team');
    await expect(lena.team('Thanks')).toHaveText('2 from the team');
    for (const text of ['Release train', 'Flaky e2e', 'on-call buddy', 'release notes']) {
      await expect(lena.page.getByText(text)).toHaveCount(0);
    }
    await expect(lena.mine('Plus')).toHaveCount(0);

    // And the other way round.
    await lena.write('Delta', 'CI takes 25 minutes');
    await expect(milo.team('Delta')).toHaveText('1 from the team');
    await expect(milo.page.getByText('CI takes 25 minutes')).toHaveCount(0);
    await expect(milo.mine('Delta')).toHaveCount(1);
  });

  test('everyone sees what is being written while an editor is open', async ({ browser }) => {
    const milo = await open(browser, 'milo');
    await expect(lena.team('Delta')).toHaveText('No reflections from the team yet');
    await expect(lena.writing('Delta')).toHaveCount(0);

    // A new reflection: a shimmering placeholder in Delta for Lena only.
    await milo.startWriting('Delta');
    await expect(lena.team('Delta')).toHaveText(
      'No reflections from the team yet · 1 being written',
    );
    await expect(lena.writing('Delta')).toHaveCount(1);
    await expect(milo.writing('Delta')).toHaveCount(0);
    await expect(lena.writing('Plus')).toHaveCount(0);

    // Cancel: nobody is writing any more.
    await milo.column('Delta').getByRole('button', { name: 'Cancel' }).click();
    await expect(lena.writing('Delta')).toHaveCount(0);
    await expect(lena.team('Delta')).toHaveText('No reflections from the team yet');

    // Editing a sent reflection counts as writing too.
    await milo.write('Ideas', 'Demo slot on Fridays');
    await expect(lena.team('Ideas')).toHaveText('1 from the team');
    await milo.column('Ideas').getByRole('button', { name: 'Edit reflection' }).click();
    await expect(lena.team('Ideas')).toHaveText('1 from the team · 1 being written');
    await expect(lena.writing('Ideas')).toHaveCount(1);
    await milo.column('Ideas').getByRole('button', { name: 'Save' }).click();
    await expect(lena.team('Ideas')).toHaveText('1 from the team');

    // Moving to another column moves the indicator; Esc closes the editor.
    await milo.startWriting('Plus');
    await expect(lena.writing('Plus')).toHaveCount(1);
    await milo.startWriting('Thanks');
    await expect(lena.writing('Plus')).toHaveCount(0);
    await expect(lena.writing('Thanks')).toHaveCount(1);
    await milo.page.keyboard.press('Escape');
    await expect(lena.writing('Thanks')).toHaveCount(0);

    // Closing the tab mid-sentence stops it as well.
    await milo.startWriting('Delta');
    await milo.page.keyboard.type('Half a thought');
    await expect(lena.team('Delta')).toContainText('1 being written');
    await milo.page.close();
    await expect(lena.team('Delta')).toHaveText('No reflections from the team yet');
    await expect(lena.writing('Delta')).toHaveCount(0);
  });

  test('authors edit, move and delete their reflections, live for the others', async ({
    browser,
  }) => {
    const milo = await open(browser, 'milo');
    await milo.write('Plus', 'Pairing helped');
    await expect(lena.team('Plus')).toHaveText('1 from the team');

    // Edit the text and move it to Ideas.
    await milo.column('Plus').getByRole('button', { name: 'Edit reflection' }).click();
    const editor = milo.editor('Plus');
    await expect(editor).toHaveText('Pairing helped');
    await editor.press('End');
    await milo.page.keyboard.type(' a lot, do more of it');
    await milo.column('Plus').getByLabel('Column').selectOption({ label: 'Ideas' });
    await milo.column('Plus').getByRole('button', { name: 'Save' }).click();
    await expect(milo.mine('Ideas')).toHaveText(/Pairing helped a lot, do more of it/);
    await expect(milo.mine('Plus')).toHaveCount(0);
    await expect(milo.column('Ideas')).toContainText('1 mine');
    await expect(lena.team('Plus')).toHaveText('No reflections from the team yet');
    await expect(lena.team('Ideas')).toHaveText('1 from the team');

    // Delete it.
    await milo.column('Ideas').getByRole('button', { name: 'Edit reflection' }).click();
    await milo.column('Ideas').getByRole('button', { name: 'Delete' }).click();
    await expect(milo.mine('Ideas')).toHaveCount(0);
    await expect(lena.team('Ideas')).toHaveText('No reflections from the team yet');

    // Nothing to send, nothing sent.
    await milo.startWriting('Delta');
    await expect(milo.column('Delta').getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  test('reflections carry rich text, sanitized', async () => {
    await lena.startWriting('Delta');
    const keyboard = lena.page.keyboard;
    const toolbar = lena.column('Delta').getByRole('toolbar', { name: 'Text formatting' });
    await toolbar.getByRole('button', { name: 'Bold' }).click();
    await keyboard.type('Bold');
    await toolbar.getByRole('button', { name: 'Bold' }).click();
    await keyboard.type(' ');
    await toolbar.getByRole('button', { name: 'Italic' }).click();
    await keyboard.type('italic');
    await toolbar.getByRole('button', { name: 'Italic' }).click();
    await keyboard.type(' ');
    await toolbar.getByRole('button', { name: 'Strikethrough' }).click();
    await keyboard.type('gone');
    await toolbar.getByRole('button', { name: 'Strikethrough' }).click();
    await keyboard.type(' ');
    await toolbar.getByRole('button', { name: 'Inline code' }).click();
    await keyboard.type('npm test');
    await toolbar.getByRole('button', { name: 'Inline code' }).click();
    await keyboard.press('Enter');
    await toolbar.getByRole('button', { name: 'Bullet list' }).click();
    await keyboard.type('first');
    await keyboard.press('Enter');
    await keyboard.type('second');
    await keyboard.press('Enter');
    await keyboard.press('Enter');
    await toolbar.getByRole('button', { name: 'Link' }).click();
    await lena.column('Delta').getByLabel('Link address').fill('example.com/runbook');
    await lena.column('Delta').getByRole('button', { name: 'Apply' }).click();
    await lena.column('Delta').getByRole('button', { name: 'Send' }).click();

    const card = lena.mine('Delta');
    await expect(card.locator('strong')).toHaveText('Bold');
    await expect(card.locator('em')).toHaveText('italic');
    await expect(card.locator('s')).toHaveText('gone');
    await expect(card.locator('code')).toHaveText('npm test');
    await expect(card.locator('ul > li')).toHaveText(['first', 'second']);
    await expect(card.getByRole('link', { name: 'https://example.com/runbook' })).toHaveAttribute(
      'href',
      'https://example.com/runbook',
    );

    // Whatever a client sends, only the allowed markup is stored and shown.
    let dialogs = 0;
    lena.page.on('dialog', (d) => {
      dialogs++;
      void d.dismiss();
    });
    await writeReflection(
      LENA,
      meetingId,
      'plus',
      '<p onclick="alert(1)">Safe<img src=x onerror="alert(1)"><script>alert(1)</script></p>',
    );
    await lena.page.reload();
    await expect(lena.mine('Plus')).toContainText('Safe');
    await expect(lena.mine('Plus').locator('img, script, [onclick], [onerror]')).toHaveCount(0);
    expect(dialogs).toBe(0);
  });

  test('reflections carry images, private to their author during Reflect', async ({ browser }) => {
    const milo = await open(browser, 'milo');
    await lena.startWriting('Thanks');
    await lena.addImage('Thanks', GIF);
    // An image alone is enough to send.
    await lena.column('Thanks').getByRole('button', { name: 'Send' }).click();

    const image = lena.mine('Thanks').locator('img[data-attachment-id]');
    await expect(image).toHaveAttribute('src', /^blob:/);
    await expect
      .poll(() => image.evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth))
      .toBe(48);

    // Milo only learns that a reflection exists.
    await expect(milo.team('Thanks')).toContainText('1 from the team');
    await expect(milo.page.locator('img[data-attachment-id]')).toHaveCount(0);
  });

  test('unsent drafts stay in the browser and are discarded at Group', async ({ browser }) => {
    const milo = await open(browser, 'milo');
    await milo.startWriting('Delta');
    await milo.page.keyboard.type('Still thinking about this');
    await expect(milo.unsentDrafts).toHaveText('1 unsent draft');
    await milo.startWriting('Ideas');
    await milo.page.keyboard.type('And this');
    await expect(milo.unsentDrafts).toHaveText('2 unsent drafts');
    // Drafts never reach the server.
    expect((await meetingState(MILO, meetingId)).reflections).toEqual([]);

    // A reload keeps them; opening the column brings the text back.
    await milo.page.reload();
    await expect(milo.phase).toHaveText('Reflect');
    await expect(milo.unsentDrafts).toHaveText('2 unsent drafts');
    await milo.startWriting('Delta');
    await expect(milo.editor('Delta')).toHaveText('Still thinking about this');

    // Cancel throws a draft away.
    await milo.column('Delta').getByRole('button', { name: 'Cancel' }).click();
    await expect(milo.unsentDrafts).toHaveText('1 unsent draft');

    // Lena moves on: the remaining draft is discarded, and Milo is told.
    await milo.write('Plus', 'Sent before Group');
    await lena.nextGroup.click();
    await expect(milo.phase).toHaveText('Group');
    await expect(
      milo.page.getByText('The meeting moved on: your unsent draft was discarded.'),
    ).toBeVisible();
    await expect(milo.unsentDrafts).toBeHidden();
    const revealed = (await meetingState(LENA, meetingId)).reflections.map((r) => r.content);
    expect(revealed).toEqual(['<p>Sent before Group</p>']);

    // Gone for good, even after a reload.
    await milo.page.reload();
    await expect(milo.phase).toHaveText('Group');
    await expect(milo.page.getByText('And this')).toHaveCount(0);
  });

  test('participants press Ready; the facilitator sees who is ready', async ({ browser }) => {
    const milo = await open(browser, 'milo');
    await expect(lena.readyCount).toContainText('0/2');
    await expect(lena.readyCount).toContainText('ready');
    await expect(lena.readyCount).toHaveAttribute('title', 'Waiting for Lena and Milo');
    // Milo is not the facilitator: no count, a hint instead.
    await expect(milo.readyCount).toBeHidden();
    await expect(milo.footer).toContainText("Lena is facilitating. Press Ready when you're done.");
    await expect(milo.nextGroup).toBeHidden();

    await expect(milo.ready).toHaveText("I'm ready");
    await expect(milo.ready).toHaveAttribute('aria-pressed', 'false');
    await milo.ready.click();
    await expect(milo.ready).toHaveText("You're ready");
    await expect(milo.ready).toHaveAttribute('aria-pressed', 'true');
    await expect(lena.readyCount).toContainText('1/2');
    await expect(lena.readyCount).toHaveAttribute('title', 'Waiting for Lena');

    // A toggle.
    await milo.ready.click();
    await expect(lena.readyCount).toContainText('0/2');
    await milo.ready.click();
    await lena.ready.click();
    await expect(lena.readyCount).toContainText('2/2');
    await expect(lena.readyCount).toHaveAttribute('title', 'Everyone is ready');

    // A new phase starts with nobody ready.
    await lena.nextGroup.click();
    await expect(milo.phase).toHaveText('Group');
    await expect(milo.ready).toHaveText("I'm ready");
    await expect(lena.readyCount).toContainText('0/2');
  });

  test('the facilitator runs the timer; everyone sees the same countdown', async ({ browser }) => {
    const milo = await open(browser, 'milo');
    await expect(milo.setTimer).toBeHidden();
    await expect(milo.timer).toBeHidden();

    // The popover: presets, a custom duration, and its help.
    await lena.setTimer.click();
    await expect(lena.setTimer).toHaveAttribute('aria-expanded', 'true');
    await expect(lena.timerDialog.getByRole('heading', { name: 'Timer' })).toBeVisible();
    await expect(lena.timerDialog).toContainText(
      "Everyone sees the same countdown. It won't move the phase on by itself.",
    );
    const presets = lena.timerDialog.getByRole('button', { name: /^\d+ min$/ });
    await expect(presets).toHaveText(['3 min', '5 min', '10 min', '15 min']);
    await expect(
      lena.timerDialog.getByRole('button', { name: '5 min', exact: true }),
    ).toHaveAttribute('aria-pressed', 'true');
    // Not a duration.
    await lena.timerDialog.getByLabel('Custom duration').fill('7:75');
    await lena.timerDialog.getByRole('button', { name: 'Start' }).click();
    await expect(lena.timerDialog.getByRole('alert')).toContainText('00:01 and 99:59');
    await expect(milo.timer).toBeHidden();
    // Esc closes the popover; the trigger gets the focus back.
    await lena.page.keyboard.press('Escape');
    await expect(lena.timerDialog).toBeHidden();
    await expect(lena.setTimer).toBeFocused();

    await lena.setTimer.click();
    await lena.timerDialog.getByRole('button', { name: '3 min', exact: true }).click();
    await expect(lena.timerDialog.getByLabel('Custom duration')).toHaveValue('03:00');
    await lena.timerDialog.getByRole('button', { name: 'Start' }).click();
    await expect(lena.timer).toHaveText(/^0[23]:\d\d$/);
    await expect(milo.timer).toHaveText(/^0[23]:\d\d$/);
    await expect(milo.timerBox).toContainText('left');
    const a = await secondsLeft(lena.timer);
    const b = await secondsLeft(milo.timer);
    expect(Math.abs(a - b)).toBeLessThanOrEqual(2);
    expect(a).toBeLessThanOrEqual(180);
    expect(a).toBeGreaterThan(170);
    // Milo has no controls.
    await expect(milo.pauseTimer).toBeHidden();
    await expect(milo.stopTimer).toBeHidden();

    // Pause: frozen, the same for everyone.
    await lena.pauseTimer.click();
    await expect(lena.resumeTimer).toBeVisible();
    await expect(milo.timerBox).toContainText('paused');
    const frozen = await secondsLeft(lena.timer);
    await expect(milo.timer).toHaveText((await lena.timer.textContent()) ?? '');
    await lena.page.waitForTimeout(1500);
    expect(await secondsLeft(lena.timer)).toBe(frozen);
    expect(await secondsLeft(milo.timer)).toBe(frozen);

    // +1 min while paused, then resume.
    await lena.addMinute.click();
    await expect.poll(() => secondsLeft(milo.timer)).toBe(frozen + 60);
    await lena.resumeTimer.click();
    await expect(milo.timerBox).toContainText('left');
    await expect.poll(() => secondsLeft(milo.timer)).toBeLessThan(frozen + 60);

    // Stop: gone for everyone.
    await lena.stopTimer.click();
    await expect(lena.setTimer).toBeVisible();
    await expect(milo.timer).toBeHidden();
    await expect(lena.timesUp).toBeHidden();
  });

  test("time's up reaches everyone and does not move the phase on", async ({ browser }) => {
    const milo = await open(browser, 'milo');
    await lena.startTimer('00:03');
    await expect(milo.timer).toBeVisible();

    await expect(milo.timesUp).toHaveText("Time's up. Lena decides when to move on.", {
      timeout: 10_000,
    });
    await expect(lena.timesUp).toHaveText("Time's up. You decide when to move on.");
    await expect(lena.timer).toHaveText('00:00');
    await expect(milo.timer).toHaveText('00:00');
    await expect(milo.phase).toHaveText('Reflect');
    await expect(lena.phase).toHaveText('Reflect');

    // Dismissed for Milo only, and it stays dismissed after a reload.
    await milo.timesUp.getByRole('button', { name: 'Dismiss' }).click();
    await expect(milo.timesUp).toBeHidden();
    await expect(lena.timesUp).toBeVisible();
    await milo.page.reload();
    await expect(milo.timer).toHaveText('00:00');
    await expect(milo.timesUp).toBeHidden();

    // One more minute restarts the countdown from now.
    await lena.addMinute.click();
    await expect(lena.timesUp).toBeHidden();
    // Milo hears of it on his own: wait for the restarted countdown.
    await expect.poll(() => secondsLeft(milo.timer)).toBeGreaterThan(50);
    // Still there in the next phase.
    await lena.nextGroup.click();
    await expect(milo.phase).toHaveText('Group');
    await expect(milo.timer).toBeVisible();
  });

  test('a late joiner gets the timer, counts and writing from the snapshot', async ({
    browser,
  }) => {
    const milo = await open(browser, 'milo');
    await writeReflection(MILO, meetingId, 'plus', '<p>Milo secret one</p>');
    await writeReflection(MILO, meetingId, 'plus', '<p>Milo secret two</p>');
    await lena.startTimer('10:00');
    await milo.startWriting('Ideas');
    await expect(lena.team('Ideas')).toContainText('1 being written');

    const nora = await open(browser, 'nora');
    await expect(nora.team('Plus')).toHaveText('2 from the team');
    await expect(nora.team('Ideas')).toHaveText(
      'No reflections from the team yet · 1 being written',
    );
    await expect(nora.writing('Ideas')).toHaveCount(1);
    await expect(nora.timer).toHaveText(/^(10:00|09:\d\d)$/);
    await expect(nora.page.getByText('Milo secret')).toHaveCount(0);
    // Joining made her a participant: she can write and be ready.
    await nora.write('Thanks', 'Thanks for having me');
    await expect(lena.team('Thanks')).toHaveText('1 from the team');
    await expect(lena.readyCount).toContainText('0/3');
  });

  test('a team admin who takes over gets the timer and Next', async ({ browser }) => {
    await setRole(LENA, teamId, await userIdOf(NORA), 'admin');
    const nora = await open(browser, 'nora');
    await expect(nora.setTimer).toBeHidden();
    await nora.adminActions.getByRole('button', { name: 'Take over facilitation' }).click();

    await expect(nora.setTimer).toBeVisible();
    await expect(nora.nextGroup).toBeVisible();
    await expect(nora.readyCount).toBeVisible();
    await expect(lena.setTimer).toBeHidden();
    await expect(lena.nextGroup).toBeHidden();
    await expect(lena.footer).toContainText("Nora is facilitating. Press Ready when you're done.");

    await nora.startTimer('05:00');
    await expect(lena.timer).toHaveText(/^0[45]:\d\d$/);
    await expect(lena.pauseTimer).toBeHidden();
  });

  test('the Reflect screen works in the dark theme', async ({ browser }) => {
    const context = await browser.newContext({
      ignoreHTTPSErrors: true,
      colorScheme: 'dark',
    });
    contexts.push(context);
    const page = await context.newPage();
    cspViolations.push(await watchCsp(page));
    await signIn(page, 'milo');
    const milo = new ReflectPage(page);
    await milo.goto(teamId, meetingId);

    // Column wells and accents come from the dark tokens.
    await expect(milo.column('Delta')).toHaveCSS('background-color', 'rgb(23, 19, 42)');
    await expect(lena.column('Delta')).toHaveCSS('background-color', 'rgb(228, 224, 238)');
    await milo.startWriting('Delta');
    await expect(milo.editor('Delta')).toBeVisible();
    await expect(milo.ready).toHaveCSS('border-top-color', 'rgb(74, 222, 128)');
    await milo.page.keyboard.type('Readable in the dark');
    await milo.page.keyboard.press('Control+Enter');
    await expect(milo.mine('Delta')).toHaveCSS('background-color', 'rgb(53, 43, 85)');
    await expect(lena.writing('Delta')).toHaveCount(0);
    await expect(lena.team('Delta')).toHaveText('1 from the team');
  });
});
