import { describe, expect, it, vi } from 'vitest';

import { me, mockFetch, OTHER_TEAM_ID, problem, renderAt, session, team } from './helpers';

describe('JoinView', () => {
  it('joins the team behind the link', async () => {
    const { calls } = mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'POST /api/v1/join/jt': {
        status: 200,
        body: { team: { id: OTHER_TEAM_ID, name: 'Platform Team' }, alreadyMember: false },
      },
      'GET /api/v1/me': me([team({ id: OTHER_TEAM_ID, role: 'member' })]),
    });
    const { wrapper } = await renderAt('/join/jt');
    expect(calls.some((c) => c.method === 'POST' && c.path === '/api/v1/join/jt')).toBe(true);
    expect(wrapper.find('h1').text()).toBe("You've joined Platform Team");
    const link = wrapper.find('a.btn--primary');
    expect(link.text()).toContain('Go to meetings');
    expect(link.attributes('href')).toBe(`/teams/${OTHER_TEAM_ID}/meetings`);
  });

  it('opens the team when the user is already a member', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'POST /api/v1/join/jt': {
        status: 200,
        body: { team: { id: OTHER_TEAM_ID, name: 'Platform Team' }, alreadyMember: true },
      },
      'GET /api/v1/me': me([team({ id: OTHER_TEAM_ID, role: 'member' })]),
    });
    const { wrapper, router } = await renderAt('/join/jt');
    await vi.waitFor(() =>
      expect(router.currentRoute.value.fullPath).toBe(`/teams/${OTHER_TEAM_ID}/meetings`),
    );
    expect(wrapper.text()).not.toContain("You've joined");
  });

  it('explains a disabled or replaced link', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'POST /api/v1/join/old': problem(404, 'join_link_invalid'),
    });
    const { wrapper } = await renderAt('/join/old');
    expect(wrapper.find('h1').text()).toBe('This join link no longer works');
    expect(wrapper.text()).toContain('An admin has turned it off or created a new one.');
  });
});
