import { expect, type Locator, type Page } from '@playwright/test';

/** `/teams/:teamId/meetings`: active meetings as cards, ended ones as rows. */
export class MeetingsPage {
  readonly heading: Locator;
  readonly addMeeting: Locator;
  readonly active: Locator;
  readonly ended: Locator;

  constructor(private readonly page: Page) {
    this.heading = page.getByRole('heading', { level: 1, name: 'Meetings' });
    this.addMeeting = page.getByRole('main').getByRole('button', { name: 'Add meeting' });
    this.active = page.getByRole('region', { name: 'Active' });
    this.ended = page.getByRole('region', { name: 'Ended' });
  }

  async goto(teamId: string): Promise<void> {
    await this.page.goto(`/teams/${teamId}/meetings`);
    await expect(this.heading).toBeVisible();
  }

  /** An active meeting's card, by title. */
  card(title: string): Locator {
    return this.active.getByRole('article').filter({
      has: this.page.getByRole('heading', { name: title, exact: true }),
    });
  }
}
