// ---------------------------------------------------------------------------
// Generates the four screenshots embedded in README.md.
//
//   npm run screenshots -- https://$(scripts/dev-host.sh)
//
// This is a doc-asset generator, not part of the e2e suite: see
// playwright.screenshots.config.ts for why it lives outside tests/e2e/.
//
// It owns the seeded users giulia, marco, sara, aisha and leo (see
// backend/src/seed.rs). Every run resets them, builds a throwaway team and
// meeting through the API, walks the meeting through Reflect, Group and
// Discuss with everyone online in their own browser context, captures the
// room and the task board, then deletes the team.
// ---------------------------------------------------------------------------
import { statSync } from 'node:fs';
import { resolve } from 'node:path';

import {
  expect,
  request,
  test,
  type Browser,
  type BrowserContext,
  type BrowserContextOptions,
  type Page,
} from '@playwright/test';

import {
  castVote,
  createTask,
  createTeam,
  deleteTeam,
  groupCards,
  joinMeeting,
  joinTeam,
  meetingAction,
  meetingState,
  moveTask,
  react,
  renameGroup,
  renameMeeting,
  resetAccount,
  setDiscussGroup,
  startMeeting,
  userIdOf,
  writeReflection,
} from '../../tests/e2e/api';
import { PASSWORD, USERS } from '../../tests/e2e/fixtures';
import { AppShell } from '../../tests/e2e/pages/AppShell';
import { DiscussPage } from '../../tests/e2e/pages/DiscussPage';
import { GroupPage } from '../../tests/e2e/pages/GroupPage';
import { ReflectPage } from '../../tests/e2e/pages/ReflectPage';
import { SignInPage } from '../../tests/e2e/pages/SignInPage';
import { TasksPage } from '../../tests/e2e/pages/TasksPage';

const OUT_DIR = 'docs/screenshots';
const SHOTS = {
  reflect: '01-reflect.png',
  group: '02-group.png',
  discuss: '03-discuss.png',
  tasks: '04-tasks.png',
} as const;

/** A blank or truncated render must never reach the repo. */
const MIN_BYTES = 8_000;
/** The `check-added-large-files` pre-commit hook rejects anything above 500 KB. */
const MAX_BYTES = 500_000;

const TEAM = 'Checkout Team';
const MEETING = 'Sprint 42 retro';

/** giulia facilitates (and is the captured page); the others take part. */
const FACILITATOR = 'giulia';
const OTHERS = ['marco', 'sara', 'aisha', 'leo'] as const;
type Login = typeof FACILITATOR | (typeof OTHERS)[number];
const EVERYONE: Login[] = [FACILITATOR, ...OTHERS];

type Column = 'plus' | 'delta' | 'ideas' | 'thanks';

/** Cards written through the API, by whom and where. */
const REFLECTIONS: { by: Login; column: Column; text: string }[] = [
  { by: 'marco', column: 'plus', text: 'Release 3.2 went out with zero rollbacks' },
  { by: 'sara', column: 'plus', text: 'Design reviews now happen before we start coding' },
  { by: 'marco', column: 'delta', text: 'CI takes 25 minutes on a good day' },
  { by: 'leo', column: 'delta', text: 'Too many night pages for alerts that could wait' },
  { by: 'aisha', column: 'delta', text: 'Standup keeps running over 20 minutes' },
  { by: 'sara', column: 'delta', text: 'Too much context switching between projects' },
  { by: 'aisha', column: 'ideas', text: 'Quarantine flaky tests automatically' },
  { by: 'marco', column: 'ideas', text: 'Split the CI pipeline by package' },
  { by: 'leo', column: 'ideas', text: 'Route low-priority alerts to a daytime queue' },
  { by: 'sara', column: 'ideas', text: 'Demo slot on Fridays' },
  { by: 'leo', column: 'ideas', text: 'Rotate the release captain every sprint' },
  { by: 'leo', column: 'thanks', text: 'Thanks Sara for covering the incident on Saturday' },
  { by: 'aisha', column: 'thanks', text: 'Thanks Leo for the new onboarding guide' },
];

/** Cards giulia types in the UI, so the Reflect shot shows her own cards. */
const MINE = {
  plus: 'Pairing on the checkout refactor paid off',
  delta: 'Flaky e2e tests blocked merges twice',
} as const;

/**
 * Groups built in the Group phase: name and the cards in it. The first card is
 * the drop target, so the group sits in its column.
 */
const GROUPS = [
  {
    name: 'Flaky CI',
    cards: [
      MINE.delta,
      'CI takes 25 minutes on a good day',
      'Quarantine flaky tests automatically',
      'Split the CI pipeline by package',
    ],
  },
  {
    name: 'On-call',
    cards: [
      'Route low-priority alerts to a daytime queue',
      'Too many night pages for alerts that could wait',
    ],
  },
  {
    name: 'Meetings',
    cards: ['Demo slot on Fridays', 'Standup keeps running over 20 minutes'],
  },
] as const;

/** Votes per participant: group name → votes (budget 5, at most 3 per group). */
const VOTES: Record<Login, Record<string, number>> = {
  giulia: { 'Flaky CI': 3, 'On-call': 2 },
  marco: { 'Flaky CI': 3, Meetings: 2 },
  sara: { 'Flaky CI': 2, 'On-call': 2, Meetings: 1 },
  aisha: { 'Flaky CI': 2, Meetings: 3 },
  leo: { 'On-call': 3, 'Flaky CI': 2 },
};

let browser: Browser;
let contexts: BrowserContext[] = [];
/** Each participant's page, kept on the meeting room so presence shows them online. */
const pages = {} as Record<Login, Page>;
let page: Page;
let teamId: string;
let meetingId: string;
let userIds: Record<Login, string>;

/** Context options: `browser.newContext` does not inherit the config's `use`. */
function contextOptions(): BrowserContextOptions {
  return {
    ignoreHTTPSErrors: true,
    // The CSP forbids inline styles; freezeChrome injects one.
    bypassCSP: true,
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 1,
    locale: 'en-US',
    timezoneId: 'Europe/Rome',
    colorScheme: 'light',
  };
}

/**
 * Fail before building anything if the deployment is not serving or does not
 * know the generator's users, with a message that names the fix.
 */
async function preflight(baseURL: string): Promise<void> {
  const api = await request.newContext({ baseURL, ignoreHTTPSErrors: true });
  try {
    let status: number;
    try {
      status = (await api.get('/api/v1/auth/config')).status();
    } catch (cause) {
      throw new Error(
        `Cannot reach ${baseURL}. Bring the cluster up first (scripts/dev.sh), then:\n` +
          '  npm run screenshots -- https://$(scripts/dev-host.sh)',
        { cause },
      );
    }
    if (status !== 200) throw new Error(`${baseURL}/api/v1/auth/config returned ${status}.`);
    const login = await api.post('/api/v1/auth/login', {
      data: { login: FACILITATOR, password: PASSWORD, remember: false },
    });
    if (!login.ok()) {
      throw new Error(
        `Cannot sign in as ${FACILITATOR} (status ${login.status()}). The screenshot users are\n` +
          'seeded by the backend in dev mode: rebuild and restart it\n' +
          '(scripts/build.sh && scripts/restart.sh).',
      );
    }
  } finally {
    await api.dispose();
  }
}

/** A mention chip of a teammate, as the editor writes it. */
function mention(login: Login): string {
  const name = USERS[login].name;
  return `<span data-type="mention" data-id="${userIds[login]}" data-label="${name}">@${name}</span>`;
}

/**
 * Hold everything still: no transitions caught mid-flight, no blinking caret,
 * no scrollbar gutters in frame.
 */
async function freezeChrome(target: Page): Promise<void> {
  await target.addStyleTag({
    content: `
      *, *::before, *::after {
        transition: none !important;
        animation: none !important;
        caret-color: transparent !important;
      }
      ::-webkit-scrollbar { width: 0 !important; height: 0 !important; display: none !important; }
    `,
  });
}

/** Wait for fonts and the last paint before capturing. */
async function settle(target: Page): Promise<void> {
  await target.evaluate(() => document.fonts.ready);
  await target.evaluate(
    () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))),
  );
}

async function capture(target: Page, name: string): Promise<void> {
  await freezeChrome(target);
  await settle(target);
  await target.screenshot({
    path: resolve(OUT_DIR, name),
    // The app shell scrolls inner regions, never the document: the viewport is
    // the screen.
    fullPage: false,
    animations: 'disabled',
    caret: 'hide',
  });
}

async function signIn(target: Page, login: Login): Promise<void> {
  const signInPage = new SignInPage(target);
  await signInPage.goto();
  await signInPage.signIn(USERS[login].username, PASSWORD);
  await expect(new AppShell(target).currentUser).toHaveText(USERS[login].name);
}

/** The id of a card, by its text. */
async function cardId(text: string): Promise<string> {
  const state = await meetingState(FACILITATOR, meetingId);
  const card = state.reflections.find((r) => r.content.includes(text));
  if (!card) throw new Error(`no card "${text}"`);
  return card.id;
}

/** The id of a named group. */
async function groupId(name: string): Promise<string> {
  const state = await meetingState(FACILITATOR, meetingId);
  const group = state.groups.find((g) => g.name === name);
  if (!group) throw new Error(`no group "${name}"`);
  return group.id;
}

/** The room as everyone sees it once the phase has changed. */
async function nextPhase(heading: string): Promise<void> {
  await meetingAction(FACILITATOR, meetingId, 'phase/next');
  for (const login of EVERYONE) {
    await expect(pages[login].getByRole('heading', { level: 1 })).toHaveText(heading);
  }
}

test.describe.configure({ mode: 'serial' });

test.beforeAll(async ({ browser: b, baseURL }) => {
  browser = b;
  await preflight(baseURL as string);

  // A previous run that failed half-way may have left its team behind.
  for (const login of EVERYONE) await resetAccount(login);

  userIds = Object.fromEntries(
    await Promise.all(EVERYONE.map(async (login) => [login, await userIdOf(login)])),
  ) as Record<Login, string>;

  teamId = await createTeam(FACILITATOR, TEAM);
  for (const login of OTHERS) await joinTeam(FACILITATOR, teamId, login);
  meetingId = await startMeeting(FACILITATOR, teamId);
  await renameMeeting(FACILITATOR, meetingId, MEETING);
  for (const login of OTHERS) await joinMeeting(login, teamId, meetingId);

  for (const login of EVERYONE) {
    const context = await browser.newContext(contextOptions());
    contexts.push(context);
    const p = await context.newPage();
    await signIn(p, login);
    await p.goto(`/teams/${teamId}/meetings/${meetingId}`);
    await expect(p.getByRole('heading', { level: 1 })).toHaveText('Icebreaker');
    pages[login] = p;
  }
  page = pages[FACILITATOR];
});

test.afterAll(async () => {
  for (const context of contexts) await context.close();
  contexts = [];
  if (teamId) await deleteTeam(FACILITATOR, teamId);
});

test('reflect', async () => {
  await nextPhase('Reflect');
  const reflect = new ReflectPage(page);

  for (const { by, column, text } of REFLECTIONS) {
    await writeReflection(by, meetingId, column, `<p>${text}</p>`);
  }
  await reflect.write('Plus', MINE.plus);
  await reflect.write('Delta', MINE.delta);

  // Two participants are done; Marco is still typing an idea.
  for (const login of ['sara', 'aisha'] as const) {
    await pages[login]
      .getByRole('contentinfo', { name: 'Meeting navigation' })
      .getByRole('button', { name: "I'm ready" })
      .click();
  }
  const marco = new ReflectPage(pages.marco);
  await marco.startWriting('Ideas');
  await pages.marco.keyboard.type('Pair on the payment');

  await reflect.startTimer('07:00');
  await reflect.pauseTimer.click();
  await expect(reflect.readyCount).toContainText('2/5');
  await expect(reflect.team('Ideas')).toContainText('1 being written');
  await expect(reflect.mine('Delta')).toHaveCount(1);

  await capture(page, SHOTS.reflect);
});

test('group', async () => {
  // Marco's unsent draft is discarded with the phase change.
  await nextPhase('Group');
  const group = new GroupPage(page);

  for (const { name, cards } of GROUPS) {
    const target = await cardId(cards[0]);
    let id = '';
    for (const text of cards.slice(1)) {
      id = await groupCards(FACILITATOR, meetingId, target, await cardId(text));
    }
    await renameGroup(FACILITATOR, id, name);
  }
  for (const { name } of GROUPS) await expect(group.group(name)).toBeVisible();
  await group.expand(group.group('On-call'));
  await page.mouse.move(0, 0);

  await capture(page, SHOTS.group);
});

test('discuss', async () => {
  await nextPhase('Vote');
  for (const login of EVERYONE) {
    for (const [name, votes] of Object.entries(VOTES[login])) {
      const id = await groupId(name);
      for (let i = 0; i < votes; i++) {
        const { status, code } = await castVote(login, id, 1);
        if (status !== 200) throw new Error(`vote ${login} → ${name}: ${status} ${code}`);
      }
    }
  }
  await nextPhase('Discuss');
  const discuss = new DiscussPage(page);
  await expect(discuss.groupTitle).toHaveText('Flaky CI');

  const reactions: [string, string, Login[]][] = [
    [MINE.delta, '👍', ['marco', 'sara', 'aisha', 'leo']],
    [MINE.delta, '😬', ['marco', 'leo']],
    ['CI takes 25 minutes on a good day', '💯', ['giulia', 'sara', 'aisha']],
    ['Quarantine flaky tests automatically', '🎉', ['marco', 'leo']],
    ['Quarantine flaky tests automatically', '👍', ['giulia', 'sara']],
    ['Split the CI pipeline by package', '❤️', ['aisha']],
  ];
  for (const [text, emoji, logins] of reactions) {
    const id = await cardId(text);
    for (const login of logins) await react(login, id, emoji);
  }

  const flaky = await groupId('Flaky CI');
  await createTask(
    'marco',
    meetingId,
    flaky,
    `<p>Quarantine flaky tests in a nightly job, ${mention('aisha')}</p>`,
  );
  await createTask(
    'giulia',
    meetingId,
    flaky,
    `<p>Split the CI pipeline by package and cache dependencies, ${mention('marco')}</p>`,
  );

  // Sara is writing a third task: the others see her placeholder.
  const sara = new DiscussPage(pages.sara);
  await sara.taskEditor.click();
  await pages.sara.keyboard.type('Track the CI duration');

  await expect(discuss.tasks.getByText('Quarantine flaky tests in a nightly job')).toBeVisible();
  await expect(discuss.writingPlaceholder('Sara')).toBeVisible();
  await expect(discuss.chip(MINE.delta, '👍')).toBeVisible();
  await page.mouse.move(0, 0);

  await capture(page, SHOTS.discuss);
});

test('tasks', async () => {
  const onCall = await groupId('On-call');
  const meetings = await groupId('Meetings');
  const flaky = await groupId('Flaky CI');

  // Tasks are written in Discuss on the current group (spec §9.5), then moved
  // on the board.
  const board: [string, Login, string, 'future' | 'stuck' | 'active' | 'done'][] = [
    [flaky, 'sara', `<p>Track the CI duration on the team dashboard</p>`, 'future'],
    [flaky, 'aisha', `<p>Upgrade the CI runners: waiting on budget approval, ${mention('giulia')}</p>`, 'stuck'],
    [onCall, 'leo', `<p>Route low-priority alerts to a daytime queue, ${mention('leo')}</p>`, 'active'],
    [onCall, 'sara', `<p>Write the on-call handover checklist</p>`, 'done'],
    [meetings, 'aisha', `<p>Time-box standup to 15 minutes, ${mention('sara')}</p>`, 'done'],
    [meetings, 'giulia', `<p>Try a Friday demo slot for a month</p>`, 'future'],
  ];
  let current = flaky;
  for (const [group, by, content, status] of board) {
    if (group !== current) {
      await setDiscussGroup(FACILITATOR, meetingId, group);
      current = group;
    }
    const id = await createTask(by, meetingId, group, content);
    if (status !== 'active') await moveTask(by, id, status);
  }

  const tasks = new TasksPage(page);
  await tasks.goto(teamId);
  await expect(tasks.count('Future')).toHaveText('2');
  await expect(tasks.count('Stuck')).toHaveText('1');
  await expect(tasks.count('Active')).toHaveText('3');
  await expect(tasks.count('Done')).toHaveText('2');
  await page.mouse.move(0, 0);

  await capture(page, SHOTS.tasks);
});

test('captured files look sane', () => {
  for (const name of Object.values(SHOTS)) {
    const { size } = statSync(resolve(OUT_DIR, name));
    expect(size, `${name} looks blank or truncated (${size} bytes)`).toBeGreaterThan(MIN_BYTES);
    expect(size, `${name} exceeds the 500 KB pre-commit limit (${size} bytes)`).toBeLessThan(
      MAX_BYTES,
    );
  }
});
