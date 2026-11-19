import { expect, type Locator, type Page } from '@playwright/test';

export type FactName = 'Date' | 'Duration' | 'Facilitator' | 'Votes cast';

/** `/teams/:teamId/meetings/:id/summary`: an ended meeting, read-only (spec §8.3). */
export class SummaryPage {
  readonly heading: Locator;
  readonly eyebrow: Locator;
  readonly endedAt: Locator;
  readonly facts: Locator;
  readonly groupsSection: Locator;
  readonly groups: Locator;
  readonly tasksSection: Locator;
  readonly tasks: Locator;
  readonly participantsSection: Locator;
  readonly participants: Locator;
  /** The participants' names, without their avatars' initials. */
  readonly participantNames: Locator;
  readonly boardLink: Locator;
  readonly backLink: Locator;
  readonly notFound: Locator;

  constructor(readonly page: Page) {
    this.heading = page.getByRole('heading', { level: 1 });
    this.eyebrow = page.getByText('Meeting summary · read-only');
    this.endedAt = page.getByTestId('ended-at');
    this.facts = page.getByLabel('Meeting details');
    this.groupsSection = page.getByRole('region', { name: 'Groups by votes' });
    this.groups = this.groupsSection.getByRole('article');
    this.tasksSection = page.getByRole('region', {
      name: 'Tasks',
      exact: true,
    });
    this.tasks = page.getByTestId('summary-task');
    this.participantsSection = page.getByRole('region', {
      name: /^Participants · \d+$/,
    });
    this.participants = this.participantsSection.getByRole('listitem');
    this.participantNames = this.participants.locator('.people__name');
    this.boardLink = this.tasksSection.getByRole('link', {
      name: 'Tasks board',
    });
    this.backLink = page.getByRole('main').getByRole('link', { name: 'Meetings', exact: true });
    this.notFound = page.getByRole('heading', {
      name: 'Summary not available',
    });
  }

  static path(teamId: string, meetingId: string): string {
    return `/teams/${teamId}/meetings/${meetingId}/summary`;
  }

  async goto(teamId: string, meetingId: string): Promise<void> {
    await this.page.goto(SummaryPage.path(teamId, meetingId));
    await expect(this.eyebrow).toBeVisible();
  }

  /** The value of one of the facts under the title. */
  fact(name: FactName): Locator {
    return this.facts
      .locator('div')
      .filter({ has: this.page.locator('dt', { hasText: name }) })
      .locator('dd');
  }

  /** A group card by its title (a name, or a lone card's text). */
  group(title: string): Locator {
    return this.groups.filter({
      has: this.page.getByRole('heading', {
        level: 3,
        name: title,
        exact: true,
      }),
    });
  }

  /** The titles of the groups, top to bottom. */
  groupTitles(): Locator {
    return this.groups.getByRole('heading', { level: 3 });
  }

  /** A group's cards. */
  cards(group: Locator): Locator {
    return group.getByRole('listitem');
  }

  /** A task by (part of) its text. */
  task(text: string): Locator {
    return this.tasks.filter({ hasText: text });
  }

  /** A task's @mention chips. */
  mentions(task: Locator): Locator {
    return task.getByRole('list', { name: 'Mentioned' }).getByRole('listitem');
  }
}
