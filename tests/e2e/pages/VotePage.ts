import { expect, type Locator, type Page } from '@playwright/test';

import { MeetingRoomPage } from './MeetingRoomPage';
import type { ColumnName } from './ReflectPage';

/** One group's vote stepper: the viewer's own votes on it. */
export interface Stepper {
  root: Locator;
  add: Locator;
  remove: Locator;
  count: Locator;
}

/** The meeting room in the Vote phase (spec §9.4). */
export class VotePage extends MeetingRoomPage {
  readonly ready: Locator;
  readonly readyCount: Locator;
  readonly nextDiscuss: Locator;
  /** "2 of 5 votes left". */
  readonly counter: Locator;
  /** "18 votes cast so far" (the facilitator only). */
  readonly castCount: Locator;
  readonly budgetBar: Locator;
  readonly settingsButton: Locator;
  readonly settingsDialog: Locator;
  readonly budgetInput: Locator;
  readonly perGroupInput: Locator;
  readonly warning: Locator;

  constructor(page: Page) {
    super(page);
    this.ready = this.footer.getByRole('button', {
      name: /I'm ready|You're ready/,
    });
    this.readyCount = this.footer.getByTestId('ready-count');
    this.nextDiscuss = this.footer.getByRole('button', {
      name: 'Next: Discuss',
    });
    this.budgetBar = page.getByTestId('vote-budget');
    this.counter = page.getByTestId('vote-left');
    this.castCount = page.getByTestId('vote-cast');
    this.settingsButton = this.facilitatorActions.getByRole('button', {
      name: 'Settings',
    });
    this.settingsDialog = page.getByRole('dialog', { name: 'Vote settings' });
    this.budgetInput = this.settingsDialog.getByRole('textbox', { name: 'Votes per person' });
    this.perGroupInput = this.settingsDialog.getByRole('textbox', { name: 'Max votes per group' });
    this.warning = this.settingsDialog.getByTestId('vote-warning');
  }

  async goto(teamId: string, meetingId: string): Promise<void> {
    await this.page.goto(`/teams/${teamId}/meetings/${meetingId}`);
    await expect(this.phase).toHaveText('Vote');
  }

  column(name: ColumnName): Locator {
    return this.page.getByRole('region', { name, exact: true });
  }

  /**
   * A group, or a never-grouped card, by its accessible name: the group's
   * name ("Untitled group" when it has none), or the card's text.
   */
  group(name: string): Locator {
    return this.page.getByTestId('vote-group').and(this.page.getByLabel(name, { exact: true }));
  }

  stepper(name: string): Stepper {
    const root = this.page.getByRole('group', {
      name: new RegExp(`^Your votes on ${escape(name)}:`),
    });
    return {
      root,
      add: root.getByRole('button', {
        name: `Add a vote to ${name}`,
        exact: true,
      }),
      remove: root.getByRole('button', {
        name: `Remove a vote from ${name}`,
        exact: true,
      }),
      count: root.getByTestId('vote-count'),
    };
  }

  /** Clicks + `n` times, each time once the server saved the vote. */
  async addVotes(name: string, n: number): Promise<void> {
    for (let i = 0; i < n; i++) {
      await Promise.all([this.voteSaved(), this.stepper(name).add.click()]);
    }
  }

  /** Clicks − `n` times, each time once the server saved it. */
  async removeVotes(name: string, n: number): Promise<void> {
    for (let i = 0; i < n; i++) {
      await Promise.all([this.voteSaved(), this.stepper(name).remove.click()]);
    }
  }

  /** Presses `key` on the focused vote button, once the server saved the vote. */
  async pressVote(key: string): Promise<void> {
    await Promise.all([this.voteSaved(), this.page.keyboard.press(key)]);
  }

  async expand(name: string): Promise<void> {
    const toggle = this.group(name).getByTestId('vote-group-toggle');
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  }

  async collapse(name: string): Promise<void> {
    const toggle = this.group(name).getByTestId('vote-group-toggle');
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  }

  /** Cards shown inside an expanded group. */
  cardsIn(name: string): Locator {
    return this.group(name).getByTestId('vote-card');
  }

  async openSettings(): Promise<void> {
    await this.settingsButton.click();
    await expect(this.settingsDialog).toBeVisible();
  }

  /** Types both settings and saves; resolves once the server saved them. */
  async saveSettings(budget: number, perGroup: number): Promise<void> {
    await this.openSettings();
    await this.budgetInput.fill(String(budget));
    await this.perGroupInput.fill(String(perGroup));
    const [saved] = await Promise.all([
      this.page.waitForResponse(
        (r) => r.request().method() === 'PATCH' && /\/settings$/.test(new URL(r.url()).pathname),
      ),
      this.settingsDialog.getByRole('button', { name: 'Save settings' }).click(),
    ]);
    expect(saved.status()).toBe(200);
    await expect(this.settingsDialog).toBeHidden();
  }

  /** The server's answer to the next vote. */
  async voteSaved(): Promise<void> {
    const res = await this.page.waitForResponse(
      (r) =>
        r.request().method() === 'POST' && /\/groups\/[^/]+\/vote$/.test(new URL(r.url()).pathname),
    );
    expect(res.status(), 'the vote is saved').toBe(200);
  }
}

function escape(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
