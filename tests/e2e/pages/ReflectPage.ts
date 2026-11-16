import { expect, type Locator, type Page } from '@playwright/test';

import { MeetingRoomPage } from './MeetingRoomPage';

export type ColumnName = 'Plus' | 'Delta' | 'Ideas' | 'Thanks';

/** The meeting room in the Reflect phase, with the bottom bar's timer and Ready. */
export class ReflectPage extends MeetingRoomPage {
  readonly setTimer: Locator;
  readonly timerDialog: Locator;
  /** The countdown, for the facilitator and everyone else alike. */
  readonly timer: Locator;
  readonly timerBox: Locator;
  readonly pauseTimer: Locator;
  readonly resumeTimer: Locator;
  readonly addMinute: Locator;
  readonly stopTimer: Locator;
  readonly ready: Locator;
  readonly readyCount: Locator;
  readonly unsentDrafts: Locator;
  readonly timesUp: Locator;
  readonly nextGroup: Locator;

  constructor(page: Page) {
    super(page);
    this.setTimer = this.footer.getByRole('button', { name: 'Set timer' });
    this.timerDialog = this.page.getByRole('dialog', { name: 'Timer' });
    this.timer = this.footer.getByRole('timer');
    this.timerBox = this.footer.getByTestId('timer');
    this.pauseTimer = this.footer.getByRole('button', { name: 'Pause timer' });
    this.resumeTimer = this.footer.getByRole('button', {
      name: 'Resume timer',
    });
    this.addMinute = this.footer.getByRole('button', {
      name: 'Add one minute',
    });
    this.stopTimer = this.footer.getByRole('button', { name: 'Stop timer' });
    this.ready = this.footer.getByRole('button', {
      name: /I'm ready|You're ready/,
    });
    this.readyCount = this.footer.getByTestId('ready-count');
    this.unsentDrafts = this.footer.getByTestId('unsent-drafts');
    this.timesUp = this.page.getByTestId('times-up');
    this.nextGroup = this.footer.getByRole('button', { name: 'Next: Group' });
  }

  async goto(teamId: string, meetingId: string): Promise<void> {
    await this.page.goto(`/teams/${teamId}/meetings/${meetingId}`);
    await expect(this.phase).toHaveText('Reflect');
  }

  column(name: ColumnName): Locator {
    return this.page.getByRole('region', { name, exact: true });
  }

  writeIn(name: ColumnName): Locator {
    return this.page.getByRole('button', {
      name: `Write a reflection in ${name}`,
    });
  }

  editor(name: ColumnName): Locator {
    return this.column(name).getByRole('textbox', {
      name: `Reflection in ${name}`,
    });
  }

  /** "4 from the team · 1 being written". */
  team(name: ColumnName): Locator {
    return this.column(name).getByTestId('team-strip').locator('[aria-live]');
  }

  /** Placeholders of reflections being written by others. */
  writing(name: ColumnName): Locator {
    return this.column(name).locator('[data-writing]');
  }

  mine(name: ColumnName): Locator {
    return this.column(name).getByTestId('my-reflection');
  }

  /** Opens the column's composer and waits for its editor to take the focus. */
  async startWriting(name: ColumnName): Promise<void> {
    await this.writeIn(name).click();
    await expect(this.editor(name)).toBeFocused();
  }

  /** Opens the column's composer, types and sends with the Send button. */
  async write(name: ColumnName, text: string): Promise<void> {
    await this.startWriting(name);
    await this.page.keyboard.type(text);
    await this.column(name).getByRole('button', { name: 'Send' }).click();
    await expect(this.mine(name).filter({ hasText: text })).toBeVisible();
  }

  /** Adds the image file at `path` to the open composer of a column. */
  async addImage(name: ColumnName, path: string): Promise<void> {
    const chooser = this.page.waitForEvent('filechooser');
    await this.column(name).getByRole('button', { name: 'Add image' }).click();
    await (await chooser).setFiles(path);
    await expect(this.column(name).locator('.rte img[data-attachment-id]')).toHaveAttribute(
      'src',
      /^blob:/,
    );
  }

  /** Starts the timer from the popover with a typed duration. */
  async startTimer(duration: string): Promise<void> {
    await this.setTimer.click();
    await expect(this.timerDialog).toBeVisible();
    await this.timerDialog.getByLabel('Custom duration').fill(duration);
    await this.timerDialog.getByRole('button', { name: 'Start' }).click();
    await expect(this.timerDialog).toBeHidden();
  }
}

/** Seconds shown by a `mm:ss` countdown. */
export async function secondsLeft(timer: Locator): Promise<number> {
  const text = (await timer.textContent())?.trim() ?? '';
  const match = /^(\d+):(\d\d)$/.exec(text);
  if (!match) throw new Error(`not a countdown: ${text}`);
  return Number(match[1]) * 60 + Number(match[2]);
}
