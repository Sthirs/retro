import { describe, expect, it, vi } from 'vitest';

import { BOB, me, mockFetch, OTHER_TEAM_ID, problem, renderAt, session, team } from './helpers';

const preview = (overrides: Record<string, unknown> = {}) => ({
  status: 200,
  body: {
    status: 'pending',
    email: 'alice@retro.local',
    expiresAt: new Date(Date.now() + 5 * 24 * 3600 * 1000).toISOString(),
    team: { id: OTHER_TEAM_ID, name: 'Platform Team' },
    invitedBy: { id: BOB.id, displayName: 'Bob Bianchi' },
    members: [{ id: BOB.id, displayName: 'Bob Bianchi' }],
    memberCount: 1,
    alreadyMember: false,
    ...overrides,
  },
});

describe('InvitationView', () => {
  it('shows the invitation', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/invitations/tok': preview(),
    });
    const { wrapper } = await renderAt('/invite/tok');

    expect(wrapper.text()).toContain('Team invitation');
    expect(wrapper.find('h1').text()).toBe('Join Platform Team');
    expect(wrapper.text()).toContain('Bob Bianchi invited you to join their team on Retro.');
    expect(wrapper.text()).toContain('Sent to');
    expect(wrapper.text()).toContain('alice@retro.local');
    expect(wrapper.text()).toContain('in 5 days');
    expect(wrapper.find('button.actions__accept').text()).toBe('Accept invitation');
  });

  it('accepts and opens the team', async () => {
    let accepted = false;
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/invitations/tok': preview(),
      'GET /api/v1/me': () => me(accepted ? [team({ id: OTHER_TEAM_ID, role: 'member' })] : []),
      'POST /api/v1/invitations/tok/accept': () => {
        accepted = true;
        return { status: 200, body: { id: OTHER_TEAM_ID, name: 'Platform Team' } };
      },
    });
    const { wrapper, router } = await renderAt('/invite/tok');
    await wrapper.find('button.actions__accept').trigger('click');
    await vi.waitFor(() =>
      expect(router.currentRoute.value.fullPath).toBe(`/teams/${OTHER_TEAM_ID}/meetings`),
    );
  });

  it('declines and goes home', async () => {
    const { calls } = mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/invitations/tok': preview(),
      'GET /api/v1/me': me(),
      'POST /api/v1/invitations/tok/decline': { status: 204 },
    });
    const { wrapper, router } = await renderAt('/invite/tok');
    await wrapper.find('button.actions__decline').trigger('click');
    await vi.waitFor(() => expect(router.currentRoute.value.name).toBe('home'));
    expect(calls.some((c) => c.path === '/api/v1/invitations/tok/decline')).toBe(true);
  });

  it('explains an expired invitation, naming the inviter and team', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/invitations/tok': preview({ status: 'expired', members: [], memberCount: 0 }),
    });
    const { wrapper } = await renderAt('/invite/tok');
    expect(wrapper.find('h1').text()).toBe('This invitation has expired');
    expect(wrapper.text()).toContain(
      'Invitations are valid for 7 days. Ask Bob Bianchi or another member of Platform Team to invite you again.',
    );
    expect(wrapper.text()).toContain('Back to Retro');
  });

  it('explains a revoked invitation', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/invitations/tok': preview({ status: 'revoked' }),
    });
    const { wrapper } = await renderAt('/invite/tok');
    expect(wrapper.find('h1').text()).toBe('This invitation was revoked');
    expect(wrapper.text()).toContain(
      'The team no longer has an open invitation for alice@retro.local.',
    );
  });

  it('handles unknown links', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/invitations/tok': problem(404, 'invitation_not_found'),
    });
    const { wrapper } = await renderAt('/invite/tok');
    expect(wrapper.find('h1').text()).toBe("This invitation link doesn't work");
  });

  it('sends members straight to the team', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/invitations/tok': preview({ alreadyMember: true }),
      'GET /api/v1/me': me([team({ id: OTHER_TEAM_ID })]),
    });
    const { router } = await renderAt('/invite/tok');
    await vi.waitFor(() => expect(router.currentRoute.value.name).toBe('team-meetings'));
  });
});
