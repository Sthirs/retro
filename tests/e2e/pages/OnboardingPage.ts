import { expect, type Locator, type Page } from '@playwright/test';

/** `/` for a user without teams. */
export class OnboardingPage {
  readonly heading: Locator;
  readonly teamName: Locator;
  readonly createTeam: Locator;
  readonly invitations: Locator;

  constructor(private readonly page: Page) {
    this.heading = page.getByRole('heading', {
      level: 1,
      name: /^Welcome to Retro/,
    });
    this.teamName = page.getByLabel('Team name');
    this.createTeam = page.getByRole('button', { name: 'Create team' });
    this.invitations = page.getByRole('region', {
      name: 'Pending invitations',
    });
  }

  async createTeamNamed(name: string): Promise<string> {
    await expect(this.heading).toBeVisible();
    await this.teamName.fill(name);
    await this.createTeam.click();
    await expect(this.page).toHaveURL(/\/teams\/[^/]+\/meetings$/);
    return this.page.url().split('/teams/')[1]!.split('/')[0]!;
  }

  invitation(team: string): Locator {
    return this.invitations.getByRole('listitem').filter({ hasText: team });
  }
}
