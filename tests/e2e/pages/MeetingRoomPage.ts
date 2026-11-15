import { expect, type Locator, type Page } from '@playwright/test';

/** `/teams/:teamId/meetings/:meetingId` while the meeting runs. */
export class MeetingRoomPage {
  readonly phase: Locator;
  readonly sidebar: Locator;
  readonly queue: Locator;
  readonly facilitatorActions: Locator;
  readonly adminActions: Locator;
  readonly footer: Locator;
  readonly nextPerson: Locator;
  readonly previousPerson: Locator;
  readonly greetingPronunciation: Locator;
  readonly transferDialog: Locator;
  readonly endDialog: Locator;
  readonly ended: Locator;

  constructor(private readonly page: Page) {
    this.phase = page.getByRole('heading', { level: 1 });
    this.sidebar = page.getByRole('navigation', { name: 'Meeting phases' });
    this.queue = page.getByRole('list', { name: 'Icebreaker progress' });
    this.facilitatorActions = page.getByRole('toolbar', { name: 'Facilitator actions' });
    this.adminActions = page.getByRole('toolbar', { name: 'Admin actions' });
    this.footer = page.getByRole('contentinfo', { name: 'Meeting navigation' });
    this.nextPerson = this.footer.getByRole('button', { name: 'Next person' });
    this.previousPerson = this.footer.getByRole('button', { name: 'Previous person' });
    this.greetingPronunciation = page
      .getByRole('region', { name: 'Icebreaker' })
      .getByText(/^\/.+\/$/);
    this.transferDialog = page.getByRole('dialog', { name: 'Transfer facilitator role' });
    this.endDialog = page.getByRole('dialog', { name: 'End this meeting?' });
    this.ended = page.getByText('This meeting has ended');
  }

  /** "Now answering · 2 of 3". */
  answering(n: number, total: number): Locator {
    return this.page.getByText(`Now answering · ${n} of ${total}`);
  }

  /** The person speaking now, in the sidebar's queue. */
  current(): Locator {
    return this.queue.locator('[aria-current="true"]');
  }

  /** The facilitator's way back to someone in the queue who had their turn. */
  goBackTo(name: string): Locator {
    return this.queue.getByRole('button', { name: `Go back to ${name}` });
  }

  async waitForIcebreaker(): Promise<void> {
    await expect(this.phase).toHaveText('Icebreaker');
  }

  async transferTo(name: string): Promise<void> {
    await this.facilitatorActions.getByRole('button', { name: 'Transfer facilitator' }).click();
    await expect(this.transferDialog).toBeVisible();
    await this.transferDialog.getByRole('radio', { name }).check();
    await this.transferDialog
      .getByRole('button', { name: `Make ${name.split(' ')[0]} facilitator` })
      .click();
    await expect(this.transferDialog).toBeHidden();
  }

  async end(): Promise<void> {
    await this.facilitatorActions.getByRole('button', { name: 'End meeting' }).click();
    await expect(this.endDialog).toBeVisible();
    await this.endDialog.getByRole('button', { name: 'End meeting' }).click();
  }
}
