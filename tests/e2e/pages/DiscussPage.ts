import { expect, type Locator, type Page } from "@playwright/test";

import { MeetingRoomPage } from "./MeetingRoomPage";

/** The meeting room in the Discuss phase (spec §9.5). */
export class DiscussPage extends MeetingRoomPage {
  /** The groups in Discuss order, in the sidebar. */
  readonly progress: Locator;
  /** The group on screen: the current one, or the one previewed. */
  readonly group: Locator;
  readonly groupTitle: Locator;
  readonly votes: Locator;
  readonly preview: Locator;
  readonly backToCurrent: Locator;
  readonly discussThis: Locator;
  readonly previousGroup: Locator;
  readonly nextGroup: Locator;
  readonly endMeeting: Locator;
  readonly tasks: Locator;
  readonly taskEditor: Locator;
  readonly addTask: Locator;
  readonly picker: Locator;
  readonly quickReactions: Locator;
  readonly search: Locator;
  readonly mentions: Locator;

  constructor(page: Page) {
    super(page);
    this.progress = page.getByRole("list", { name: "Discuss progress" });
    this.group = page.getByTestId("discuss-group");
    this.groupTitle = this.group.getByRole("heading", { level: 2 });
    this.votes = page.getByTestId("discuss-votes");
    this.preview = page.getByTestId("discuss-preview");
    this.backToCurrent = page.getByRole("button", {
      name: "Back to current group",
    });
    this.discussThis = page.getByRole("button", { name: "Discuss this group" });
    this.previousGroup = this.footer.getByRole("button", {
      name: "Previous group",
    });
    this.nextGroup = this.footer.getByRole("button", { name: "Next group" });
    this.endMeeting = this.footer.getByRole("button", { name: "End meeting" });
    this.tasks = page.getByRole("complementary", {
      name: "Tasks for this group",
    });
    this.taskEditor = this.tasks.getByRole("textbox", { name: "New task" });
    this.addTask = this.tasks.getByRole("button", { name: "Add task" });
    this.picker = page.getByRole("dialog", { name: "Add a reaction" });
    this.quickReactions = this.picker.getByTestId("quick-reaction");
    this.search = this.picker.getByRole("textbox", { name: "Search emoji" });
    this.mentions = page.getByRole("listbox");
  }

  async goto(teamId: string, meetingId: string): Promise<void> {
    await this.page.goto(`/teams/${teamId}/meetings/${meetingId}`);
    await expect(this.phase).toHaveText("Discuss");
  }

  /** A group's entry in the sidebar, by its name (or a lone card's text). */
  entry(name: string): Locator {
    return this.progress.getByRole("button").filter({ hasText: name });
  }

  /** "Group 2 of 3". */
  groupOf(n: number, total: number): Locator {
    return this.group.getByText(`Group ${n} of ${total}`, { exact: true });
  }

  /** A card on screen, by its text. */
  card(text: string): Locator {
    return this.group.getByRole("article").filter({ hasText: text });
  }

  /** A card's chip for one emoji ("👍 reaction, 4"). */
  chip(text: string, emoji: string): Locator {
    return this.card(text).getByRole("button", {
      name: new RegExp(`^${emoji} reaction, \\d+$`, "u"),
    });
  }

  /** Where each card on screen sits on the page (not the viewport, which scrolls), in order. */
  async cardBoxes(): Promise<
    { x: number; y: number; width: number; height: number }[]
  > {
    return this.group.getByRole("article").evaluateAll((cards) =>
      cards.map((c) => {
        const r = c.getBoundingClientRect();
        return {
          x: r.x + scrollX,
          y: r.y + scrollY,
          width: r.width,
          height: r.height,
        };
      }),
    );
  }

  addReaction(text: string): Locator {
    return this.card(text).getByRole("button", { name: "Add reaction" });
  }

  /** Opens a card's picker and waits for the emoji to load. */
  async openPicker(text: string): Promise<void> {
    await this.addReaction(text).click();
    await expect(this.picker).toBeVisible();
    await expect(
      this.picker.getByRole("button", {
        name: "emoji Grinning face",
        exact: true,
      }),
    ).toBeVisible();
  }

  /** Reacts to a card with an emoji from the picker's grid, and waits for the server. */
  async reactFromPicker(text: string, emojiName: string): Promise<void> {
    await this.openPicker(text);
    await this.saved(
      "PUT",
      this.picker
        .getByRole("button", { name: `emoji ${emojiName}`, exact: true })
        .click(),
    );
    await expect(this.picker).toBeHidden();
  }

  /** Clicks a chip and waits for the server to take the toggle. */
  async toggle(text: string, emoji: string): Promise<void> {
    const chip = this.chip(text, emoji);
    const pressed = await chip.getAttribute("aria-pressed");
    await this.saved(pressed === "true" ? "DELETE" : "PUT", chip.click());
  }

  /** Runs `action` and waits for the reaction request it sends to succeed. */
  async saved(method: "PUT" | "DELETE", action: Promise<void>): Promise<void> {
    const [res] = await Promise.all([
      this.page.waitForResponse(
        (r) =>
          r.request().method() === method &&
          /\/reactions\/[^/]+$/.test(new URL(r.url()).pathname),
      ),
      action,
    ]);
    expect(res.status()).toBe(200);
  }

  /** Writes a task (Ctrl+Enter is not used: the button, as most people do). */
  async writeTask(text: string): Promise<void> {
    await this.taskEditor.click();
    await this.page.keyboard.type(text);
    const [res] = await Promise.all([
      this.page.waitForResponse(
        (r) =>
          r.request().method() === "POST" &&
          new URL(r.url()).pathname.endsWith("/tasks"),
      ),
      this.addTask.click(),
    ]);
    expect(res.status()).toBe(201);
  }

  /** The tasks listed for the group on screen. */
  taskItems(): Locator {
    return this.tasks.getByRole("listitem");
  }

  /** A task on screen, by its text. */
  taskItem(text: string): Locator {
    return this.taskItems().filter({ hasText: text });
  }

  /** A task's Edit (pencil) button. */
  editTask(text: string): Locator {
    return this.taskItem(text).getByRole("button", { name: /^Edit task: / });
  }

  /** The editor open in place on a task. */
  taskEditorOf(text: string): Locator {
    return this.taskItem(text).getByRole("textbox", { name: "Edit task" });
  }

  /** "Walt is writing a task…", for someone else's unsent task. */
  writingPlaceholder(firstName: string): Locator {
    return this.tasks.getByRole("status", {
      name: `${firstName} is writing a task`,
    });
  }

  /** Runs `action` and waits for the task request it sends (`PATCH` or `DELETE`). */
  async taskSaved(
    method: "PATCH" | "DELETE",
    action: Promise<void>,
  ): Promise<void> {
    const [res] = await Promise.all([
      this.page.waitForResponse(
        (r) =>
          r.request().method() === method &&
          /\/tasks\/[^/]+$/.test(new URL(r.url()).pathname),
      ),
      action,
    ]);
    expect(res.status()).toBe(method === "PATCH" ? 200 : 204);
  }

  /** Deletes a task with its trash button, once confirmed. */
  async deleteTask(text: string): Promise<void> {
    await this.taskItem(text)
      .getByRole("button", { name: /^Delete task: / })
      .click();
    const dialog = this.page.getByRole("dialog", { name: "Delete this task?" });
    await expect(dialog).toBeVisible();
    await this.taskSaved(
      "DELETE",
      dialog.getByRole("button", { name: "Delete" }).click(),
    );
    await expect(dialog).toBeHidden();
  }
}
