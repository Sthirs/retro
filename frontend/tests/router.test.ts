import { beforeEach, describe, expect, it } from 'vitest';

import {
  ALICE,
  authConfig,
  me,
  mockFetch,
  OTHER_TEAM_ID,
  session,
  setup,
  team,
  TEAM_ID,
  unauthorized,
} from './helpers';

describe('router guards', () => {
  beforeEach(() => localStorage.clear());

  it('sends signed-out users to /login with the requested path', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig(),
    });
    const { router } = setup();
    await router.push('/teams/42/meetings');
    expect(router.currentRoute.value.fullPath).toBe('/login?redirect=/teams/42/meetings');

    await router.push('/invite/abc');
    expect(router.currentRoute.value.query.redirect).toBe('/invite/abc');

    await router.push('/');
    expect(router.currentRoute.value.fullPath).toBe('/login');
  });

  it('restores the session from the refresh cookie on first navigation', async () => {
    const { calls } = mockFetch({ 'POST /api/v1/auth/refresh': session(), 'GET /api/v1/me': me() });
    const { router } = setup();
    await router.push('/');
    expect(router.currentRoute.value.name).toBe('home');
    expect(calls.filter((c) => c.path === '/api/v1/auth/refresh')).toHaveLength(1);

    await router.push('/login');
    expect(router.currentRoute.value.name).toBe('home');
  });

  it('sends signed-in users away from /login to a safe redirect', async () => {
    mockFetch({ 'POST /api/v1/auth/refresh': session(), 'GET /api/v1/me': me() });
    const { router } = setup();
    await router.push('/login?redirect=//evil.example');
    expect(router.currentRoute.value.fullPath).toBe('/');
  });

  it('opens the first team when the user has teams', async () => {
    mockFetch({ 'POST /api/v1/auth/refresh': session(), 'GET /api/v1/me': me([team()]) });
    const { router } = setup();
    await router.push('/');
    expect(router.currentRoute.value.fullPath).toBe(`/teams/${TEAM_ID}/meetings`);
  });

  it('remembers the last used team per user', async () => {
    const second = team({ id: OTHER_TEAM_ID, name: 'Mobile Squad' });
    mockFetch({ 'POST /api/v1/auth/refresh': session(), 'GET /api/v1/me': me([team(), second]) });
    const { router } = setup();
    await router.push(`/teams/${OTHER_TEAM_ID}/settings`);
    expect(localStorage.getItem(`retro.lastTeam.${ALICE.id}`)).toBe(OTHER_TEAM_ID);
    await router.push('/');
    expect(router.currentRoute.value.fullPath).toBe(`/teams/${OTHER_TEAM_ID}/meetings`);
  });

  it('keeps users out of teams they do not belong to', async () => {
    mockFetch({ 'POST /api/v1/auth/refresh': session(), 'GET /api/v1/me': me() });
    const { router } = setup();
    await router.push(`/teams/${TEAM_ID}/settings`);
    expect(router.currentRoute.value.name).toBe('home');
  });
});
