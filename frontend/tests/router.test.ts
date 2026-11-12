import { describe, expect, it } from 'vitest';

import { authConfig, mockFetch, session, setup, unauthorized } from './helpers';

describe('router guards', () => {
  it('sends signed-out users to /login with the requested path', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig(),
    });
    const { router } = setup();
    await router.push('/teams/42/meetings');
    // Unknown paths redirect to home, which requires a session.
    expect(router.currentRoute.value.name).toBe('login');

    await router.push('/');
    expect(router.currentRoute.value.fullPath).toBe('/login');
  });

  it('restores the session from the refresh cookie on first navigation', async () => {
    const { calls } = mockFetch({ 'POST /api/v1/auth/refresh': session() });
    const { router } = setup();
    await router.push('/');
    expect(router.currentRoute.value.name).toBe('home');
    expect(calls.filter((c) => c.path === '/api/v1/auth/refresh')).toHaveLength(1);

    await router.push('/login');
    expect(router.currentRoute.value.name).toBe('home');
  });

  it('sends signed-in users away from /login to a safe redirect', async () => {
    mockFetch({ 'POST /api/v1/auth/refresh': session() });
    const { router } = setup();
    await router.push('/login?redirect=//evil.example');
    expect(router.currentRoute.value.fullPath).toBe('/');
  });
});
