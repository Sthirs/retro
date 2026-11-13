import { createPinia, setActivePinia } from 'pinia';
import { vi } from 'vitest';
import { createMemoryHistory, type Router } from 'vue-router';

import { buildI18n } from '@/i18n';
import { buildRouter } from '@/router';

export interface FakeRoute {
  status: number;
  body?: unknown;
}

/** Routes `fetch` calls by "METHOD path" to canned JSON responses. */
export function mockFetch(routes: Record<string, FakeRoute | (() => FakeRoute)>) {
  const calls: { method: string; path: string; body: unknown; headers: Headers }[] = [];
  const fetchMock = vi.fn(async (input: Request) => {
    const url = new URL(input.url);
    const key = `${input.method} ${url.pathname}`;
    const text = await input.clone().text();
    calls.push({
      method: input.method,
      path: url.pathname + url.search,
      body: text ? JSON.parse(text) : undefined,
      headers: input.headers,
    });
    const route = routes[key];
    const resolved = typeof route === 'function' ? route() : route;
    if (!resolved) return new Response(null, { status: 404 });
    return new Response(resolved.body === undefined ? null : JSON.stringify(resolved.body), {
      status: resolved.status,
      headers: { 'Content-Type': 'application/json' },
    });
  });
  vi.stubGlobal('fetch', fetchMock);
  return { fetchMock, calls };
}

export const ALICE = {
  id: '0190a4e0-0000-7000-8000-000000000001',
  email: 'alice@retro.local',
  username: 'alice',
  displayName: 'Alice Rossi',
  avatarUrl: null,
};

export const session = (user = ALICE) => ({
  status: 200,
  body: { accessToken: 'access-token', expiresIn: 900, user },
});

export const unauthorized = {
  status: 401,
  body: { type: 'about:blank', title: 'Unauthorized', status: 401, code: 'unauthorized' },
};

export const authConfig = (overrides: Record<string, unknown> = {}) => ({
  status: 200,
  body: {
    localLogin: true,
    sso: { enabled: true, buttonText: 'Continue with single sign-on' },
    passwordResetUrl: null,
    ...overrides,
  },
});

export function setup(): { router: Router; plugins: unknown[] } {
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = buildRouter(createMemoryHistory());
  return { router, plugins: [pinia, buildI18n(), router] };
}
