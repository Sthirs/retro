import { expect, type Locator, type Page } from '@playwright/test';

export type ColumnName = 'Future' | 'Stuck' | 'Active' | 'Done';

/** `/teams/:teamId/tasks`: the board of four columns. */
export class TasksPage {
  readonly heading: Locator;
  readonly myTasks: Locator;
  readonly mentioned: Locator;
  readonly author: Locator;
  readonly menu: Locator;
  readonly editDialog: Locator;
  readonly deleteDialog: Locator;

  constructor(private readonly page: Page) {
    this.heading = page.getByRole('heading', { level: 1, name: 'Tasks' });
    this.myTasks = page.getByRole('button', { name: 'My tasks' });
    this.mentioned = page.getByRole('combobox', { name: 'Mentioned' });
    this.author = page.getByRole('combobox', { name: 'Author' });
    this.menu = page.getByRole('menu', { name: 'Task actions' });
    this.editDialog = page.getByRole('dialog', { name: 'Edit task' });
    this.deleteDialog = page.getByRole('dialog', { name: 'Delete this task?' });
  }

  async goto(teamId: string): Promise<void> {
    await this.page.goto(`/teams/${teamId}/tasks`);
    await expect(this.heading).toBeVisible();
  }

  column(name: ColumnName): Locator {
    return this.page.getByRole('region', { name, exact: true });
  }

  /** The count pill in a column's header. */
  count(name: ColumnName): Locator {
    return this.column(name).getByTestId('count');
  }

  /** A card by (part of) its text, on the whole board. */
  card(text: string): Locator {
    return this.page.getByRole('article').filter({ hasText: text });
  }

  cards(name: ColumnName): Locator {
    return this.column(name).getByRole('article');
  }

  /** The "Task actions" button of a card. */
  actions(cardText: string): Locator {
    return this.card(cardText).getByRole('button', { name: 'Task actions' });
  }

  /**
   * A real pointer drag (SortableJS reacts to pointer moves, not to the
   * synthetic HTML5 drag events of `locator.dragTo`). The card is dropped on the
   * `target` box at `fraction` of its height (measured once the drag has started).
   */
  async drag(source: Locator, target: Locator, fraction = 0.5): Promise<void> {
    await source.scrollIntoViewIfNeeded();
    const from = (await source.boundingBox())!;
    const startX = from.x + from.width / 2;
    const startY = from.y + from.height / 2;
    await this.page.mouse.move(startX, startY);
    await this.page.mouse.down();
    // A first nudge past SortableJS's drag threshold starts the drag. The card
    // being dragged shrinks to a placeholder, which shifts the cards below it:
    // wait for that, and only then measure the target.
    await this.page.mouse.move(startX + 4, startY + 4, { steps: 3 });
    await expect(this.page.locator('.column__ghost')).toBeVisible();
    await target.scrollIntoViewIfNeeded();
    const to = (await target.boundingBox())!;
    await this.page.mouse.move(to.x + to.width / 2, to.y + to.height * fraction, { steps: 20 });
    await this.page.mouse.up();
  }

  /** Drops a card onto the empty end of a column (its list), after its last card. */
  async dragToColumn(cardText: string, name: ColumnName): Promise<void> {
    await this.drag(this.card(cardText), this.column(name).locator('.column__list'));
  }

  /** The keyboard route: Task actions, Move to…, then the column. */
  async moveWithKeyboard(cardText: string, name: ColumnName): Promise<void> {
    await this.actions(cardText).focus();
    await this.page.keyboard.press('Enter');
    await expect(this.menu).toBeVisible();
    await this.menu.getByRole('menuitem', { name: 'Move to…' }).click();
    await this.page.getByRole('menu', { name: 'Move to…' }).getByRole('menuitem', { name }).click();
  }

  async filterMine(): Promise<void> {
    await this.myTasks.click();
    await expect(this.myTasks).toHaveAttribute('aria-pressed', 'true');
  }

  async edit(cardText: string, appendedText: string): Promise<void> {
    await this.actions(cardText).click();
    await this.menu.getByRole('menuitem', { name: 'Edit' }).click();
    await expect(this.editDialog).toBeVisible();
    const editor = this.editDialog.getByRole('textbox', { name: 'Task' });
    await editor.click();
    await this.page.keyboard.press('ControlOrMeta+End');
    await this.page.keyboard.type(appendedText);
    await this.editDialog.getByRole('button', { name: 'Save' }).click();
    await expect(this.editDialog).toBeHidden();
  }

  async delete(cardText: string): Promise<void> {
    await this.actions(cardText).click();
    await this.menu.getByRole('menuitem', { name: 'Delete' }).click();
    await expect(this.deleteDialog).toBeVisible();
    await this.deleteDialog.getByRole('button', { name: 'Delete' }).click();
    await expect(this.deleteDialog).toBeHidden();
  }
}
