import { describe, expect, it } from 'vitest';

import { api } from '@/api/client';
import { useSessionStore } from '@/stores/session';

import { ALICE, mockFetch, session, setup, unauthorized } from './helpers';

describe('session store', () => {
  it('shares one refresh between concurrent callers', async () => {
    const { calls } = mockFetch({ 'POST /api/v1/auth/refresh': session() });
    setup();
    const store = useSessionStore();
    const results = await Promise.all([store.refresh(), store.refresh(), store.refresh()]);
    expect(results).toEqual([true, true, true]);
    expect(calls).toHaveLength(1);
    expect(store.user?.email).toBe(ALICE.email);
  });

  it('clears the user when the refresh cookie is rejected', async () => {
    mockFetch({ 'POST /api/v1/auth/refresh': unauthorized });
    setup();
    const store = useSessionStore();
    expect(await store.refresh()).toBe(false);
    expect(store.isAuthenticated).toBe(false);
  });

  it('sends the bearer token and retries once after renewing it', async () => {
    let meCalls = 0;
    const { calls } = mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'GET /api/v1/me': () =>
        ++meCalls === 1 ? unauthorized : { status: 200, body: { user: ALICE } },
    });
    setup();
    const store = useSessionStore();
    store.accessToken = 'expired';

    const res = await api.me();
    expect(res.status).toBe(200);
    expect(res.data).toEqual({ user: ALICE });
    const me = calls.filter((c) => c.path === '/api/v1/me');
    expect(me[0]?.headers.get('Authorization')).toBe('Bearer expired');
    expect(me[1]?.headers.get('Authorization')).toBe('Bearer access-token');
  });

  it('signs out even when the network fails', async () => {
    mockFetch({ 'POST /api/v1/auth/refresh': session() });
    setup();
    const store = useSessionStore();
    await store.refresh();
    await store.logout();
    expect(store.user).toBeNull();
    expect(store.accessToken).toBeNull();
  });
});
