import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  ALICE,
  me,
  mockFetch,
  OTHER_TEAM_ID,
  pendingInvitation,
  renderAt,
  session,
  team,
  TEAM_ID,
  type FakeRoute,
} from './helpers';

const second = team({ id: OTHER_TEAM_ID, name: 'Mobile Squad', role: 'member' });

function base(extra: Record<string, FakeRoute | (() => FakeRoute)> = {}) {
  return {
    'POST /api/v1/auth/refresh': session(),
    'GET /api/v1/me': me([team(), second]),
    ...extra,
  };
}

describe('app shell', () => {
  beforeEach(() => localStorage.clear());

  it('shows the team, the left menu and the signed-in user', async () => {
    mockFetch(base());
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/meetings`);
    expect(wrapper.find('[data-testid="team-selector"]').text()).toContain('Platform Team');
    expect(wrapper.find('[data-testid="current-user"]').text()).toBe(ALICE.displayName);
    const current = wrapper.find('nav a[aria-current="page"]');
    expect(current.text()).toBe('Meetings');
    expect(wrapper.find('nav').text()).toContain('Team settings');
  });

  it('switches team, staying in the same section', async () => {
    mockFetch(base());
    const { wrapper, router } = await renderAt(`/teams/${TEAM_ID}/tasks`);
    await wrapper.find('[data-testid="team-selector"]').trigger('click');
    const item = wrapper
      .findAll('[role="menuitem"]')
      .find((i) => i.text().includes('Mobile Squad'));
    await item?.trigger('click');
    await vi.waitFor(() =>
      expect(router.currentRoute.value.fullPath).toBe(`/teams/${OTHER_TEAM_ID}/tasks`),
    );
  });

  it('creates a team from "Add team"', async () => {
    const created = team({ id: '0190a4e0-0000-7000-8000-0000000000a3', name: 'Design Guild' });
    let teams = [team(), second];
    mockFetch(
      base({
        'GET /api/v1/me': () => me(teams),
        'POST /api/v1/teams': () => {
          teams = [...teams, created];
          return { status: 201, body: created };
        },
      }),
    );
    const { wrapper, router } = await renderAt(`/teams/${TEAM_ID}/meetings`);
    await wrapper.find('[data-testid="team-selector"]').trigger('click');
    await wrapper
      .findAll('[role="menuitem"]')
      .find((i) => i.text() === 'Add team')
      ?.trigger('click');
    await vi.waitFor(() => expect(wrapper.find('#new-team').exists()).toBe(true));
    expect(wrapper.text()).toContain(
      "You'll be the first admin. You can invite people once the team exists.",
    );
    await wrapper.find('#new-team').setValue('Design Guild');
    await wrapper.find('#create-team-form').trigger('submit');
    await vi.waitFor(() =>
      expect(router.currentRoute.value.fullPath).toBe(`/teams/${created.id}/meetings`),
    );
  });

  it('lists pending invitations in the user menu', async () => {
    mockFetch(base({ 'GET /api/v1/me': me([team()], [pendingInvitation()]) }));
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/meetings`);
    const bell = wrapper.find('button[aria-label="Pending invitations, 1 new"]');
    expect(bell.exists()).toBe(true);
    await bell.trigger('click');
    const panel = wrapper.find('#user-menu');
    expect(panel.isVisible()).toBe(true);
    expect(panel.text()).toContain('Pending invitations · 1');
    expect(panel.text()).toContain('Mobile Squad');
    expect(panel.text()).toContain('from Bob Bianchi');
    expect(panel.find('button[aria-label="Accept invitation to Mobile Squad"]').exists()).toBe(
      true,
    );
    expect(panel.text()).toContain('Log out');
  });

  it('switches the theme from the user menu', async () => {
    mockFetch(base());
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/meetings`);
    await wrapper.find('button[aria-label="User menu"]').trigger('click');
    const dark = wrapper.findAll('#user-menu button').find((b) => b.text() === 'Dark');
    await dark?.trigger('click');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(dark?.attributes('aria-pressed')).toBe('true');
    document.documentElement.removeAttribute('data-theme');
  });

  it('logs out', async () => {
    mockFetch(
      base({
        'POST /api/v1/auth/logout': { status: 204 },
        'GET /api/v1/auth/config': {
          status: 200,
          body: {
            localLogin: true,
            sso: { enabled: false, buttonText: '' },
            passwordResetUrl: null,
          },
        },
      }),
    );
    const { wrapper, router } = await renderAt(`/teams/${TEAM_ID}/meetings`);
    await wrapper.find('button[aria-label="User menu"]').trigger('click');
    await wrapper
      .findAll('#user-menu button')
      .find((b) => b.text() === 'Log out')
      ?.trigger('click');
    await vi.waitFor(() => expect(router.currentRoute.value.name).toBe('login'));
  });
});
