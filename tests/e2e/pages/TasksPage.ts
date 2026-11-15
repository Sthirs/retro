import { expect, type Locator, type Page } from '@playwright/test';

export type ColumnName = 'Future' | 'Stuck' | 'Active' | 'Done';
export type FilterName = 'mentioned' | 'author' | 'meeting';

/** `/teams/:teamId/tasks`: the board of four columns. */
export class TasksPage {
  readonly heading: Locator;
  readonly myTasks: Locator;
  readonly menu: Locator;
  readonly editDialog: Locator;
  readonly deleteDialog: Locator;
  /** Shown once the board has loaded and has no tasks. */
  readonly empty: Locator;

  constructor(private readonly page: Page) {
    this.empty = page.getByText(
      'No tasks yet. Tasks are created in the Discuss phase of a meeting.',
    );
    this.heading = page.getByRole('heading', { level: 1, name: 'Tasks' });
    this.myTasks = page.getByRole('button', { name: 'My tasks' });
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

  /**
   * The whole keyboard route, no pointer: Enter on "Task actions", ArrowDown to
   * "Move to…", ArrowRight into its submenu, ArrowDown to the column, Enter.
   * Focus must end up on the target column.
   */
  async moveWithKeyboard(cardText: string, name: ColumnName): Promise<void> {
    await this.actions(cardText).focus();
    await this.page.keyboard.press('Enter');
    await expect(this.menu).toBeVisible();
    // Opening the menu focuses its first item (Edit); one ArrowDown reaches Move to….
    await expect(this.menu.getByRole('menuitem', { name: 'Edit' })).toBeFocused();
    await this.page.keyboard.press('ArrowDown');
    await expect(this.menu.getByRole('menuitem', { name: 'Move to…' })).toBeFocused();
    await this.page.keyboard.press('ArrowRight');

    const submenu = this.page.getByRole('menu', { name: 'Move to…' });
    const items = submenu.getByRole('menuitem');
    const labels = await items.allTextContents();
    const index = labels.map((label) => label.trim()).indexOf(name);
    expect(index, `${name} is offered by the submenu`).toBeGreaterThanOrEqual(0);
    // The submenu opens on its first item.
    await expect(items.first()).toBeFocused();
    for (let i = 0; i < index; i++) await this.page.keyboard.press('ArrowDown');
    await expect(items.nth(index)).toBeFocused();
    await this.page.keyboard.press('Enter');

    await expect(this.menu).toBeHidden();
    await expect(this.column(name)).toBeFocused();
  }

  /** A filter dropdown: its chip and, while open, its list. */
  filter(name: FilterName): Locator {
    return this.page.getByTestId(`filter-${name}`);
  }

  /** The chip button that opens a filter's list. */
  filterButton(name: FilterName): Locator {
    return this.filter(name).locator('button[aria-haspopup="listbox"]');
  }

  /**
   * Opens a filter and picks options by (part of) their label. Mentioned takes
   * several and is closed with Done; the others close on their choice.
   */
  async pick(name: FilterName, ...labels: string[]): Promise<void> {
    await this.filterButton(name).click();
    const list = this.filter(name).getByRole('listbox');
    await expect(list).toBeVisible();
    for (const label of labels) await list.getByRole('option', { name: label }).click();
    if (name === 'mentioned') {
      await this.filter(name).getByRole('button', { name: 'Done' }).click();
    }
    await expect(list).toBeHidden();
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
