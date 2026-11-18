import { expect, type Locator, type Page } from '@playwright/test';

import { MeetingRoomPage } from './MeetingRoomPage';
import type { ColumnName } from './ReflectPage';

/** The meeting room in the Group phase (spec §9.3). */
export class GroupPage extends MeetingRoomPage {
  readonly ready: Locator;
  readonly nextVote: Locator;
  /** The copy of a card that follows the pointer while this tab drags it. */
  readonly ghost: Locator;

  constructor(page: Page) {
    super(page);
    this.ready = this.footer.getByRole('button', {
      name: /I'm ready|You're ready/,
    });
    this.nextVote = this.footer.getByRole('button', { name: 'Next: Vote' });
    this.ghost = page.locator('[data-drag-ghost]');
  }

  async goto(teamId: string, meetingId: string): Promise<void> {
    await this.page.goto(`/teams/${teamId}/meetings/${meetingId}`);
    await expect(this.phase).toHaveText('Group');
  }

  column(name: ColumnName): Locator {
    return this.page.getByRole('region', { name, exact: true });
  }

  /** A card, by its text. */
  card(text: string): Locator {
    return this.page.getByRole('article').filter({ hasText: text });
  }

  /** A group, by its name ("Untitled group" when it has none). */
  group(name: string): Locator {
    return this.page.getByTestId('group').and(this.page.getByRole('region', { name, exact: true }));
  }

  groupName(group: Locator): Locator {
    return group.getByRole('textbox', { name: 'Group name' });
  }

  moveTo(text: string): Locator {
    return this.card(text).getByRole('button', { name: 'Move to…' });
  }

  /** "Aisha is moving", on a card someone else drags. */
  badge(text: string): Locator {
    return this.card(text).getByTestId('drag-badge');
  }

  async expand(group: Locator): Promise<void> {
    await group.getByRole('button', { name: 'Expand' }).click();
    await expect(group.getByRole('button', { name: 'Collapse group' })).toBeVisible();
  }

  async rename(group: Locator, name: string): Promise<void> {
    const saved = this.saved('PATCH', /\/api\/v1\/groups\/[^/]+$/);
    await this.groupName(group).fill(name);
    await this.groupName(group).press('Enter');
    await saved;
  }

  /** Presses on a card and moves past the drag threshold: the card is lifted. */
  async pickUp(text: string): Promise<void> {
    const source = this.card(text);
    await source.scrollIntoViewIfNeeded();
    const box = (await source.boundingBox())!;
    const x = box.x + box.width / 2;
    const y = box.y + 20;
    await this.page.mouse.move(x, y);
    await this.page.mouse.down();
    await this.page.mouse.move(x + 10, y + 10, { steps: 4 });
    await expect(this.ghost).toBeVisible();
  }

  /** Moves the lifted card over `target` and drops it there. */
  async dropOn(target: Locator): Promise<void> {
    await target.scrollIntoViewIfNeeded();
    const box = (await target.boundingBox())!;
    await this.page.mouse.move(box.x + box.width / 2, box.y + Math.min(24, box.height / 2), {
      steps: 15,
    });
    await this.page.mouse.up();
    await expect(this.ghost).toHaveCount(0);
  }

  /** A real pointer drag of a card onto a card, a group or a column; resolves once saved. */
  async drag(text: string, target: Locator): Promise<void> {
    const saved = this.saved(
      undefined,
      /\/api\/v1\/(meetings\/[^/]+\/groups|reflections\/[^/]+\/group)$/,
    );
    await this.pickUp(text);
    await this.dropOn(target);
    await saved;
  }

  /**
   * The keyboard route: focus the card's Move to…, Enter, ArrowDown to the
   * item, Enter. Resolves once the server saved the move.
   */
  async moveWithKeyboard(text: string, item: string): Promise<void> {
    await this.moveTo(text).focus();
    await this.page.keyboard.press('Enter');
    const menu = this.page.getByRole('menu', { name: 'Move to…' });
    await expect(menu).toBeVisible();
    const items = menu.getByRole('menuitem');
    await expect(items.first()).toBeFocused();
    const labels = (await items.allTextContents()).map((label) => label.trim());
    const index = labels.findIndex((label) => label.startsWith(item));
    expect(index, `${item} is offered by the menu`).toBeGreaterThanOrEqual(0);
    for (let i = 0; i < index; i++) await this.page.keyboard.press('ArrowDown');
    await expect(items.nth(index)).toBeFocused();
    const saved = this.saved(
      undefined,
      /\/api\/v1\/(meetings\/[^/]+\/groups|reflections\/[^/]+\/group)$/,
    );
    await this.page.keyboard.press('Enter');
    await saved;
  }

  /** The server's answer to the next matching request. */
  private async saved(method: string | undefined, url: RegExp): Promise<void> {
    const res = await this.page.waitForResponse(
      (r) => (!method || r.request().method() === method) && url.test(new URL(r.url()).pathname),
    );
    expect(res.status(), 'the change is saved').toBe(200);
  }
}
