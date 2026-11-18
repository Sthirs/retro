import path from "node:path";

import { expect, test, type Browser, type Page, type Request } from "@playwright/test";

import { createTeam, deleteTeam, discussMeeting, seedTask, userIdOf } from "../api";
import { watchCsp } from "../csp";
import { PASSWORD, USERS } from "../fixtures";
import { channelSubscribed } from "../realtime";
import { AccountPage } from "../pages/AccountPage";
import { AppShell } from "../pages/AppShell";
import { SignInPage } from "../pages/SignInPage";
import { TasksPage } from "../pages/TasksPage";
import { TeamSettingsPage } from "../pages/TeamSettingsPage";

const TEAM = "Tasks E2E";
const ERIN = USERS.erin.username;
const FINN = USERS.finn.username;

const GIF = path.join(__dirname, "..", "assets", "anim.gif");

const PIN = "Pin it";
const STUCK = "Erin stuck item";
const FUTURE = ["Future one", "Future two", "Future three"];

// This spec owns the erin and finn accounts, so it never races the other specs.
test.describe.configure({ mode: "serial" });

async function signIn(page: Page, user: keyof typeof USERS): Promise<AppShell> {
  const signInPage = new SignInPage(page);
  await signInPage.goto();
  await signInPage.signIn(USERS[user].username, PASSWORD);
  const shell = new AppShell(page);
  await expect(shell.currentUser).toHaveText(USERS[user].name);
  return shell;
}

test.describe("tasks board", () => {
  let teamId: string;
  let cspViolations: string[];

  // Each test works in a team of its own and deletes only that team, so it
  // needs no particular starting state for erin and finn.
  test.beforeEach(async ({ page }) => {
    teamId = await createTeam(ERIN, TEAM);
    cspViolations = await watchCsp(page);
    await signIn(page, "erin");
  });

  test.afterEach(async () => {
    await deleteTeam(ERIN, teamId);
    // The board works within the app's own Content Security Policy.
    expect(cspViolations).toEqual([]);
  });

  /** Finn joins through the team's join link, in his own browser session. */
  async function finnJoins(page: Page, browser: Browser): Promise<Page> {
    const settings = new TeamSettingsPage(page);
    await settings.goto(teamId);
    await expect(settings.joinSwitch).toHaveAttribute("aria-checked", "true");
    const joinLink = await settings.joinLink.inputValue();

    const finn = await (await browser.newContext()).newPage();
    await signIn(finn, "finn");
    await finn.goto(new URL(joinLink).pathname);
    await expect(finn.getByRole("heading", { level: 1 })).toHaveText(
      `You've joined ${TEAM}`,
    );
    return finn;
  }

  test("board: drag, reorder, move, filter, and a member edits and deletes", async ({
    page,
    browser,
  }) => {
    const finn = await finnJoins(page, browser);
    const finnId = await userIdOf(FINN);

    // Four tasks (plus two more in Future, so a column can be reordered).
    await seedTask(ERIN, teamId, {
      status: "active",
      content: `<p>${PIN} <span data-type="mention" data-id="${finnId}" data-label="${USERS.finn.name}">@${USERS.finn.name}</span></p>`,
    });
    await seedTask(FINN, teamId, {
      status: "future",
      content: `<p>${FUTURE[0]}</p>`,
    });
    for (const text of FUTURE.slice(1)) {
      await seedTask(ERIN, teamId, {
        status: "future",
        content: `<p>${text}</p>`,
      });
    }
    await seedTask(ERIN, teamId, {
      status: "stuck",
      content: `<p>${STUCK}</p>`,
    });

    // Erin sees all four columns with their counts.
    const tasks = new TasksPage(page);
    await tasks.goto(teamId);
    for (const name of ["Future", "Stuck", "Active", "Done"] as const) {
      await expect(tasks.column(name)).toBeVisible();
    }
    await expect(tasks.count("Future")).toHaveText("3");
    await expect(tasks.count("Stuck")).toHaveText("1");
    await expect(tasks.count("Active")).toHaveText("1");
    await expect(tasks.count("Done")).toHaveText("0");
    await expect(tasks.card(PIN)).toContainText(`@${USERS.finn.name}`);

    // The actions menu of the last card of the tallest column is not clipped, even
    // on a short screen with that card at the bottom edge.
    await page.setViewportSize({ width: 1280, height: 560 });
    const last = tasks.card(FUTURE[2]!);
    await last.evaluate((el) => el.scrollIntoView({ block: "end" }));
    await expect(last).toBeInViewport();
    await tasks.actions(FUTURE[2]!).click();
    await expect(
      tasks.menu.getByRole("menuitem", { name: "Edit" }),
    ).toBeInViewport({ ratio: 1 });
    await expect(
      tasks.menu.getByRole("menuitem", { name: "Delete" }),
    ).toBeInViewport({ ratio: 1 });
    await page.keyboard.press("Escape");
    await expect(tasks.menu).toBeHidden();
    await page.setViewportSize({ width: 1280, height: 720 });

    // A real drag from Active to Done survives a reload.
    await tasks.dragToColumn(PIN, "Done");
    await expect(tasks.count("Done")).toHaveText("1");
    await expect(tasks.cards("Done")).toContainText([PIN]);
    await page.reload();
    await expect(tasks.count("Done")).toHaveText("1");
    await expect(tasks.cards("Done")).toContainText([PIN]);
    await expect(tasks.count("Active")).toHaveText("0");

    // A downward drag within Future: the first card goes below the second.
    await expect(tasks.cards("Future")).toContainText(FUTURE);
    await tasks.drag(tasks.card(FUTURE[0]!), tasks.card(FUTURE[1]!), 0.9);
    await expect(tasks.cards("Future")).toContainText([
      FUTURE[1]!,
      FUTURE[0]!,
      FUTURE[2]!,
    ]);
    await page.reload();
    await expect(tasks.cards("Future")).toContainText([
      FUTURE[1]!,
      FUTURE[0]!,
      FUTURE[2]!,
    ]);

    // Move to… driven by the keyboard alone.
    await tasks.moveWithKeyboard(STUCK, "Active");
    await expect(tasks.cards("Active")).toContainText([STUCK]);
    await expect(tasks.count("Stuck")).toHaveText("0");
    await page.reload();
    await expect(tasks.cards("Active")).toContainText([STUCK]);

    // Finn, a plain member, edits a card Erin wrote, and Erin sees the new text.
    const finnTasks = new TasksPage(finn);
    await finnTasks.goto(teamId);
    await expect(finn.getByRole("article")).toHaveCount(5);
    await finnTasks.edit(STUCK, " (edited)");
    await expect(finnTasks.card(STUCK)).toContainText(`${STUCK} (edited)`);
    await page.reload();
    await expect(tasks.card(STUCK)).toContainText(`${STUCK} (edited)`);

    // …then deletes it, and it is gone for Erin too.
    await finnTasks.delete(STUCK);
    await expect(finnTasks.card(STUCK)).toHaveCount(0);
    await page.reload();
    await expect(tasks.heading).toBeVisible();
    await expect(tasks.card(STUCK)).toHaveCount(0);
    await expect(page.getByRole("article")).toHaveCount(4);

    // Finn's "My tasks" shows only the card that mentions him.
    await finnTasks.filterMine();
    await expect(finn.getByRole("article")).toHaveCount(1);
    await expect(finnTasks.cards("Done")).toContainText([PIN]);
    await expect(finn).toHaveURL(new RegExp(`mentioned=`));
    await finn.context().close();
  });

  test("filters: Mentioned takes several people, Meeting narrows the board", async ({
    page,
    browser,
  }) => {
    const finn = await finnJoins(page, browser);
    await finn.context().close();
    const [erinId, finnId] = await Promise.all([
      userIdOf(ERIN),
      userIdOf(FINN),
    ]);
    const chip = (id: string, name: string) =>
      `<span data-type="mention" data-id="${id}" data-label="${name}">@${name}</span>`;
    const first = await discussMeeting(ERIN, teamId);
    const second = await discussMeeting(ERIN, teamId);
    await seedTask(ERIN, teamId, {
      status: "future",
      content: `<p>For Finn ${chip(finnId, USERS.finn.name)}</p>`,
      meeting: first,
    });
    await seedTask(ERIN, teamId, {
      status: "stuck",
      content: `<p>For Erin ${chip(erinId, USERS.erin.name)}</p>`,
      meeting: second,
    });
    await seedTask(ERIN, teamId, {
      status: "active",
      content: "<p>For nobody</p>",
      meeting: second,
    });

    const tasks = new TasksPage(page);
    await tasks.goto(teamId);
    await expect(page.getByRole("article")).toHaveCount(3);

    // Two people: tasks that mention either of them.
    await tasks.pick("mentioned", USERS.erin.name, USERS.finn.name);
    await expect(tasks.filterButton("mentioned")).toHaveText(
      `Mentioned: me, ${USERS.finn.name.split(" ")[0]}`,
    );
    await expect(page.getByRole("article")).toHaveCount(2);
    await expect(tasks.card("For nobody")).toHaveCount(0);
    await expect(page).toHaveURL(/mentioned=[^&]+(%2C|,)/);
    await expect(tasks.myTasks).toHaveAttribute("aria-pressed", "false");

    // The chip's ✕ clears only that filter.
    await tasks
      .filter("mentioned")
      .getByRole("button", { name: "Clear Mentioned filter" })
      .click();
    await expect(page.getByRole("article")).toHaveCount(3);

    await tasks.pick("meeting", "Meeting 2");
    await expect(tasks.filterButton("meeting")).toHaveText(
      "Meeting: Meeting 2",
    );
    await expect(page.getByRole("article")).toHaveCount(2);
    await expect(tasks.card("For Finn")).toHaveCount(0);

    await page.getByRole("button", { name: "Clear filters" }).click();
    await expect(page.getByRole("article")).toHaveCount(3);
    await expect(page).not.toHaveURL(/meeting=/);

    // On a phone, a list opened near the right edge stays on screen.
    await page.setViewportSize({ width: 390, height: 844 });
    await tasks.filterButton("meeting").click();
    await expect(tasks.filter("meeting").getByRole("listbox")).toBeVisible();
    await expect
      .poll(() =>
        page.evaluate(() => ({
          scrollX: window.scrollX,
          overflow:
            document.documentElement.scrollWidth - document.documentElement.clientWidth,
        })),
      )
      .toEqual({ scrollX: 0, overflow: 0 });
  });

  test("live: another member's changes appear without a reload", async ({
    page,
    browser,
  }) => {
    const finn = await finnJoins(page, browser);
    const finnTasks = new TasksPage(finn);
    await finnTasks.goto(teamId);
    // The first load has answered (the empty state), so the seeded task can
    // only reach Finn through the team channel.
    await expect(finnTasks.empty).toBeVisible();
    await expect(finn.getByRole("article")).toHaveCount(0);

    // Erin's new task shows up on Finn's open board.
    await seedTask(ERIN, teamId, { status: "future", content: "<p>Live one</p>" });
    await expect(finnTasks.card("Live one")).toBeVisible();

    // And Finn's move reaches Erin's board.
    const tasks = new TasksPage(page);
    await tasks.goto(teamId);
    await expect(tasks.card("Live one")).toBeVisible();
    await finnTasks.moveWithKeyboard("Live one", "Done");
    await expect(tasks.cards("Done")).toHaveCount(1);
    await finn.context().close();
  });

  test("images: a GIF added to a task is shown, as WebP, to the whole team", async ({
    page,
    browser,
  }) => {
    await seedTask(ERIN, teamId, { status: "active", content: "<p>Look at this</p>" });
    const finn = await finnJoins(page, browser);
    const tasks = new TasksPage(page);
    await tasks.goto(teamId);

    const served = page.waitForResponse(
      (res) => /\/api\/v1\/attachments\/[0-9a-f-]+$/.test(new URL(res.url()).pathname),
    );
    await tasks.addImage("Look at this", GIF);
    const image = tasks.images("Look at this");
    await expect(image).toHaveAttribute("src", /^blob:/);
    await expect(image).toHaveAttribute("width", "48");
    await expect(image).toHaveAttribute("height", "32");
    // Converted on upload, and served so that it cannot be sniffed or run.
    const headers = (await served).headers();
    expect(headers["content-type"]).toBe("image/webp");
    expect(headers["x-content-type-options"]).toBe("nosniff");
    expect(headers["content-security-policy"]).toBe("default-src 'none'; sandbox");
    await expect
      .poll(() => image.evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth))
      .toBe(48);

    // Finn, another member, sees it too.
    const finnTasks = new TasksPage(finn);
    await finnTasks.goto(teamId);
    await expect(finnTasks.images("Look at this")).toHaveAttribute("src", /^blob:/);
    await finn.context().close();
  });

  test("live: the board does not reload for its own changes", async ({ page, browser }) => {
    const finn = await finnJoins(page, browser);
    await seedTask(ERIN, teamId, { status: "future", content: "<p>Mine to move</p>" });
    await seedTask(FINN, teamId, { status: "future", content: "<p>Finn's own</p>" });
    const isList = (request: Request) =>
      request.method() === "GET" &&
      /^\/api\/v1\/teams\/[^/]+\/tasks$/.test(new URL(request.url()).pathname);
    // A board loaded before its channel went live re-reads itself once it is
    // (it may have missed events). Let any such re-read land before counting.
    let inFlight = 0;
    page.on("request", (r) => void (isList(r) && inFlight++));
    page.on("requestfinished", (r) => void (isList(r) && inFlight--));
    page.on("requestfailed", (r) => void (isList(r) && inFlight--));
    const subscribed = channelSubscribed(page, `team:${teamId}`);
    const tasks = new TasksPage(page);
    await tasks.goto(teamId);
    const finnTasks = new TasksPage(finn);
    await finnTasks.goto(teamId);
    await expect(tasks.card("Mine to move")).toBeVisible();
    await expect(finnTasks.card("Mine to move")).toBeVisible();
    await subscribed;
    await expect.poll(() => inFlight).toBe(0);

    let reloads = 0;
    page.on("request", (request) => {
      if (isList(request)) reloads++;
    });

    // Erin's move and edit reach Finn, and Erin's board does not reload for them.
    await tasks.moveWithKeyboard("Mine to move", "Done");
    await expect(finnTasks.cards("Done")).toHaveCount(1);
    await tasks.edit("Mine to move", " now");
    await expect(finnTasks.card("Mine to move now")).toBeVisible();
    expect(reloads).toBe(0);

    // Finn's change still reloads Erin's board, once.
    await finnTasks.moveWithKeyboard("Finn's own", "Active");
    await expect(tasks.cards("Active")).toHaveCount(1);
    expect(reloads).toBe(1);
    await finn.context().close();
  });

  test("touch: a quick swipe over a card scrolls and never starts a drag", async ({
    browser,
  }) => {
    // Phone-like context: touch events, 390 px wide.
    const context = await browser.newContext({
      hasTouch: true,
      isMobile: true,
      viewport: { width: 390, height: 844 },
      ignoreHTTPSErrors: true,
    });
    try {
      const phone = await context.newPage();
      await signIn(phone, "erin");

      // Tall enough to scroll: three columns of tall-ish cards.
      const texts: string[] = [];
      for (const status of ["future", "stuck", "active", "done"] as const) {
        for (let i = 0; i < 4; i++) {
          const text = `${status} card ${i}`;
          texts.push(text);
          await seedTask(ERIN, teamId, { status, content: `<p>${text}</p>` });
        }
      }
      const tasks = new TasksPage(phone);
      await tasks.goto(teamId);
      await expect(phone.getByRole("article")).toHaveCount(texts.length);

      const snapshot = async () => ({
        Future: await tasks.cards("Future").allInnerTexts(),
        Stuck: await tasks.cards("Stuck").allInnerTexts(),
        Active: await tasks.cards("Active").allInnerTexts(),
        Done: await tasks.cards("Done").allInnerTexts(),
      });
      const before = await snapshot();

      // A real touch swipe (finger moves up), dispatched through CDP: Playwright
      // has no swipe API. It starts on the second card of the first column and
      // ends well within the 200 ms drag delay.
      const box = (await tasks.cards("Future").nth(1).boundingBox())!;
      const x = box.x + box.width / 2;
      const y = Math.min(box.y + box.height / 2, 700);
      const scrollBefore = await phone.evaluate(() => window.scrollY);
      const cdp = await context.newCDPSession(phone);
      const touch = (
        type: "touchStart" | "touchMove" | "touchEnd",
        ty?: number,
      ) =>
        cdp.send("Input.dispatchTouchEvent", {
          type,
          touchPoints: ty === undefined ? [] : [{ x, y: ty }],
        });
      await touch("touchStart", y);
      for (let step = 1; step <= 8; step++) {
        await touch("touchMove", y - step * 30);
      }
      await touch("touchEnd");

      // The page scrolled: the swipe was a scroll, not a drag.
      await expect
        .poll(() => phone.evaluate(() => window.scrollY))
        .toBeGreaterThan(scrollBefore);
      expect(await snapshot()).toEqual(before);
      await phone.reload();
      await expect(phone.getByRole("article")).toHaveCount(texts.length);
      expect(await snapshot()).toEqual(before);
    } finally {
      await context.close();
    }
  });

  for (const theme of ["Light", "Dark"] as const) {
    test(`mobile: no horizontal scroll at 390 px in the ${theme.toLowerCase()} theme`, async ({
      page,
    }) => {
      for (const [status, text] of [
        ["future", FUTURE[0]!],
        ["stuck", STUCK],
        ["active", PIN],
        ["done", "Shipped it"],
      ] as const) {
        await seedTask(ERIN, teamId, { status, content: `<p>${text}</p>` });
      }
      const account = new AccountPage(page);
      await account.goto();
      await account.chooseTheme(theme);
      await page.setViewportSize({ width: 390, height: 844 });
      await expect(page.locator("html")).toHaveAttribute(
        "data-theme",
        theme.toLowerCase(),
      );

      const tasks = new TasksPage(page);
      await tasks.goto(teamId);
      await expect(tasks.card(PIN)).toBeVisible();
      await expect
        .poll(() =>
          page.evaluate(
            () =>
              document.documentElement.clientWidth -
              document.documentElement.scrollWidth,
          ),
        )
        .toBeGreaterThanOrEqual(0);
    });
  }
});
