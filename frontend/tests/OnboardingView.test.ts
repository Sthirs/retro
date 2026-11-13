import { describe, expect, it, vi } from 'vitest';

import {
  me,
  mockFetch,
  OTHER_TEAM_ID,
  pendingInvitation,
  problem,
  renderAt,
  session,
  team,
} from './helpers';

describe('OnboardingView', () => {
  it('welcomes a user without teams and lists their invitations', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/me': me([], [pendingInvitation()]),
    });
    const { wrapper } = await renderAt('/');

    expect(wrapper.find('h1').text()).toBe('Welcome to Retro, Alice');
    expect(wrapper.text()).toContain(
      "You're not in a team yet. Create one, or accept an invitation from a team you've been invited to.",
    );
    expect(wrapper.find('label[for="team-name"]').text()).toBe('Team name');
    expect(wrapper.text()).toContain('Sent to alice@retro.local');
    expect(wrapper.text()).toContain('Mobile Squad');
    expect(wrapper.text()).toContain('Invited by Bob Bianchi · expires in 6 days');
    expect(wrapper.text()).toContain('Got an invite link instead? Just open it.');
  });

  it('creates a team and opens it', async () => {
    let teams: unknown[] = [];
    const { calls } = mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/me': () => me(teams),
      'POST /api/v1/teams': () => {
        teams = [team({ memberCount: 1 })];
        return { status: 201, body: team({ memberCount: 1 }) };
      },
    });
    const { wrapper, router } = await renderAt('/');

    await wrapper.find('#team-name').setValue('Platform Team');
    await wrapper.find('form').trigger('submit');
    await vi.waitFor(() => expect(router.currentRoute.value.name).toBe('team-meetings'));

    expect(calls.find((c) => c.method === 'POST' && c.path === '/api/v1/teams')?.body).toEqual({
      name: 'Platform Team',
    });
  });

  it('validates the team name', async () => {
    const { calls } = mockFetch({ 'POST /api/v1/auth/refresh': session(), 'GET /api/v1/me': me() });
    const { wrapper } = await renderAt('/');
    await wrapper.find('form').trigger('submit');
    expect(wrapper.find('#team-name').attributes('aria-invalid')).toBe('true');
    expect(wrapper.text()).toContain('Enter a team name of up to 60 characters.');
    expect(calls.some((c) => c.method === 'POST' && c.path === '/api/v1/teams')).toBe(false);
  });

  it('accepts an invitation and lands in the team', async () => {
    let accepted = false;
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/me': () =>
        accepted
          ? me([team({ id: OTHER_TEAM_ID, name: 'Mobile Squad', role: 'member' })])
          : me([], [pendingInvitation()]),
      'POST /api/v1/invitations/tok-mobile/accept': () => {
        accepted = true;
        return { status: 200, body: { id: OTHER_TEAM_ID, name: 'Mobile Squad' } };
      },
    });
    const { wrapper, router } = await renderAt('/');
    await wrapper.find('button[aria-label="Accept invitation to Mobile Squad"]').trigger('click');
    await vi.waitFor(() =>
      expect(router.currentRoute.value.fullPath).toBe(`/teams/${OTHER_TEAM_ID}/meetings`),
    );
  });

  it('explains why an invitation could not be accepted', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/me': me([], [pendingInvitation()]),
      'POST /api/v1/invitations/tok-mobile/accept': problem(410, 'invitation_revoked'),
    });
    const { wrapper } = await renderAt('/');
    await wrapper.find('button[aria-label="Accept invitation to Mobile Squad"]').trigger('click');
    await vi.waitFor(() =>
      expect(wrapper.find('[role="alert"]').text()).toContain('This invitation was revoked.'),
    );
  });
});
