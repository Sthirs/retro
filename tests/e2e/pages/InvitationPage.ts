import type { Locator, Page } from '@playwright/test';

/** `/invite/:token` and `/join/:joinToken` outcomes. */
export class InvitationPage {
  readonly heading: Locator;
  readonly accept: Locator;
  readonly decline: Locator;

  constructor(page: Page) {
    this.heading = page.getByRole('heading', { level: 1 });
    this.accept = page.getByRole('button', { name: 'Accept invitation' });
    this.decline = page.getByRole('button', { name: 'Decline' });
  }
}
