import { expect, type Locator, type Page } from '@playwright/test';

/** `/teams/:teamId/settings`. */
export class TeamSettingsPage {
  readonly heading: Locator;
  readonly subtitle: Locator;
  readonly teamName: Locator;
  readonly save: Locator;
  readonly inviteEmail: Locator;
  readonly createInvitation: Locator;
  readonly inviteDialog: Locator;
  readonly joinLink: Locator;
  readonly joinSwitch: Locator;
  readonly regenerate: Locator;
  readonly members: Locator;
  readonly pendingInvitations: Locator;

  constructor(private readonly page: Page) {
    this.heading = page.getByRole('heading', {
      level: 1,
      name: 'Team settings',
    });
    this.subtitle = this.heading.locator('xpath=following-sibling::p');
    this.teamName = page.getByRole('textbox', { name: 'Team name' });
    this.save = page.getByRole('button', { name: 'Save' });
    this.inviteEmail = page.getByRole('textbox', { name: 'Invite by email' });
    this.createInvitation = page
      .getByRole('region', { name: 'Invite people' })
      .getByRole('button', { name: 'Create invitation' });
    this.inviteDialog = page.getByRole('dialog', { name: 'Invite by email' });
    this.joinLink = page.getByRole('textbox', { name: 'Join link' });
    this.joinSwitch = page.getByRole('switch', {
      name: 'Anyone with the link can join as a member',
    });
    this.regenerate = page.getByRole('button', { name: 'Regenerate link' });
    this.members = page.getByTestId('member');
    this.pendingInvitations = page.getByTestId('pending-invitation');
  }

  async goto(teamId: string): Promise<void> {
    await this.page.goto(`/teams/${teamId}/settings`);
    await expect(this.heading).toBeVisible();
  }

  member(name: string): Locator {
    return this.members.filter({ hasText: name });
  }

  /** Creates an invitation and returns its personal link. */
  async invite(email: string): Promise<string> {
    await this.inviteEmail.fill(email);
    await this.createInvitation.click();
    await expect(this.inviteDialog.getByText('Invitation created')).toBeVisible();
    const link = await this.inviteDialog
      .getByRole('textbox', { name: 'Invitation link' })
      .inputValue();
    await this.inviteDialog.getByRole('button', { name: 'Done' }).click();
    await expect(this.inviteDialog).toBeHidden();
    return link;
  }

  async deleteTeam(name: string): Promise<void> {
    await this.page.getByRole('button', { name: 'Delete team', exact: true }).click();
    const dialog = this.page.getByRole('dialog', { name: `Delete ${name}?` });
    const confirm = dialog.getByRole('button', { name: 'Delete team forever' });
    await expect(confirm).toBeDisabled();
    await dialog.getByLabel('Type the team name to confirm').fill(name);
    await confirm.click();
  }
}
