import { flushPromises, mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import type { Plugin } from 'vue';
import { vi } from 'vitest';
import { createMemoryHistory, type Router } from 'vue-router';

import App from '@/App.vue';
import { buildI18n } from '@/i18n';
import { buildRouter } from '@/router';

export interface FakeRoute {
  status: number;
  body?: unknown;
}

/** Routes `fetch` calls by "METHOD path" to canned responses (JSON, or a `Blob` body as is). */
export function mockFetch(
  routes: Record<
    string,
    FakeRoute | ((request: { url: URL; body: unknown }) => FakeRoute | Promise<FakeRoute>)
  >,
) {
  const calls: { method: string; path: string; body: unknown; headers: Headers }[] = [];
  const fetchMock = vi.fn(async (input: Request) => {
    const url = new URL(input.url);
    const key = `${input.method} ${url.pathname}`;
    const isJson = input.headers.get('Content-Type')?.includes('json') ?? false;
    let body: unknown;
    if (isJson) {
      const text = await input.clone().text();
      body = text ? JSON.parse(text) : undefined;
    } else {
      const bytes = await input.clone().arrayBuffer();
      body = bytes.byteLength > 0 ? bytes : undefined;
    }
    calls.push({
      method: input.method,
      path: url.pathname + url.search,
      body,
      headers: input.headers,
    });
    const route = routes[key];
    const resolved = typeof route === 'function' ? await route({ url, body }) : route;
    if (!resolved) return new Response(null, { status: 404 });
    if (resolved.body instanceof Blob) {
      return new Response(resolved.body, {
        status: resolved.status,
        headers: { 'Content-Type': resolved.body.type || 'application/octet-stream' },
      });
    }
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

export const BOB = {
  id: '0190a4e0-0000-7000-8000-000000000002',
  email: 'bob@retro.local',
  username: 'bob',
  displayName: 'Bob Bianchi',
  avatarUrl: null,
};

export const TEAM_ID = '0190a4e0-0000-7000-8000-0000000000a1';
export const OTHER_TEAM_ID = '0190a4e0-0000-7000-8000-0000000000a2';

export const team = (overrides: Record<string, unknown> = {}) => ({
  id: TEAM_ID,
  name: 'Platform Team',
  role: 'admin',
  memberCount: 2,
  ...overrides,
});

export const pendingInvitation = (overrides: Record<string, unknown> = {}) => ({
  token: 'tok-mobile',
  team: { id: OTHER_TEAM_ID, name: 'Mobile Squad' },
  invitedBy: { id: BOB.id, displayName: 'Bob Bianchi' },
  expiresAt: new Date(Date.now() + 6 * 24 * 3600 * 1000).toISOString(),
  ...overrides,
});

/** `GET /me` with the given teams and invitations. */
export const me = (
  teams: unknown[] = [],
  invitations: unknown[] = [],
  user: typeof ALICE = ALICE,
  extra: Record<string, unknown> = {},
) => ({
  status: 200,
  body: {
    user,
    teams,
    invitations,
    memberSince: '2026-09-29T10:00:00Z',
    signInMethod: 'password',
    ...extra,
  },
});

export const problem = (status: number, code: string) => ({
  status,
  body: { type: 'about:blank', title: 'Error', status, code },
});

/** Mounts the whole app at `path`, after the router settled. */
export async function renderAt(path: string) {
  const { router, plugins } = setup();
  await router.push(path);
  await router.isReady();
  const wrapper = mount(App, { global: { plugins: plugins as Plugin[] }, attachTo: document.body });
  await flushPromises();
  return { wrapper, router };
}

/** `POST /ws-ticket`, for views that open the real-time socket. */
export const wsTicket = () => ({
  status: 200,
  body: { ticket: 'ws-ticket', expiresAt: new Date(Date.now() + 30_000).toISOString() },
});

/** A fake WebSocket: tests open it, push server messages and read what was sent. */
export class MockWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  static instances: MockWebSocket[] = [];

  readyState = MockWebSocket.CONNECTING;
  sent: { type: string; channel: string; [key: string]: unknown }[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;

  constructor(public readonly url: string) {
    MockWebSocket.instances.push(this);
  }

  /** The newest socket. */
  static get last(): MockWebSocket | undefined {
    return MockWebSocket.instances[MockWebSocket.instances.length - 1];
  }

  send(text: string): void {
    this.sent.push(JSON.parse(text));
  }

  close(): void {
    if (this.readyState === MockWebSocket.CLOSED) return;
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.();
  }

  /** The server accepted the connection. */
  open(): void {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
  }

  /** A message from the server. */
  emit(message: Record<string, unknown>): void {
    this.onmessage?.({ data: JSON.stringify({ seq: null, payload: {}, ...message }) });
  }

  /** The connection dropped. */
  drop(): void {
    this.close();
  }

  /** Channels subscribed and not unsubscribed since. */
  get channels(): string[] {
    const open = new Set<string>();
    for (const m of this.sent) {
      if (m.type === 'subscribe') open.add(m.channel);
      if (m.type === 'unsubscribe') open.delete(m.channel);
    }
    return [...open];
  }
}

/** Replaces `WebSocket` with `MockWebSocket` and forgets the app socket's state. */
export async function mockSocket(): Promise<void> {
  const { realtime } = await import('@/api/realtime');
  realtime.disconnect();
  MockWebSocket.instances = [];
  vi.stubGlobal('WebSocket', MockWebSocket);
}

/** Opens the app's socket once it has been created. */
export async function openSocket(): Promise<MockWebSocket> {
  await flushPromises();
  const socket = MockWebSocket.last;
  if (!socket) throw new Error('no WebSocket was opened');
  if (socket.readyState !== MockWebSocket.OPEN) socket.open();
  await flushPromises();
  return socket;
}

export const MEETING_ID = '0190a4e0-0000-7000-8000-0000000000c1';

/** A participant of a meeting. */
export const participant = (user: typeof ALICE, overrides: Record<string, unknown> = {}) => ({
  id: user.id,
  displayName: user.displayName,
  pictureVersion: null,
  role: 'member',
  online: true,
  ready: false,
  ...overrides,
});

/** `GET /meetings/{id}/state`: alice facilitates, alice then bob speak. */
export const meetingState = (overrides: Record<string, unknown> = {}) => ({
  id: MEETING_ID,
  teamId: TEAM_ID,
  teamName: 'Platform Team',
  title: 'Retro #3',
  status: 'active',
  phase: 'icebreaker',
  facilitatorId: ALICE.id,
  participants: [participant(ALICE, { role: 'admin' }), participant(BOB)],
  onlineIds: [ALICE.id, BOB.id],
  greeting: { text: 'Ciao!', language: 'Italian', langCode: 'it', pronunciation: '/ˈtʃa.o/' },
  question: "What's a small thing that made your week better?",
  icebreaker: { order: [ALICE.id, BOB.id], index: 0 },
  taskCount: 0,
  timer: null,
  reflections: [],
  reflectionCounts: { plus: 0, delta: 0, ideas: 0, thanks: 0 },
  writing: {
    plus: { new: 0, edit: 0 },
    delta: { new: 0, edit: 0 },
    ideas: { new: 0, edit: 0 },
    thanks: { new: 0, edit: 0 },
  },
  groups: [],
  drags: [],
  voteBudget: 5,
  voteMaxPerGroup: 3,
  myVotes: [],
  votesCast: null,
  seq: 2,
  createdAt: '2026-11-10T17:02:00Z',
  endedAt: null,
  serverNow: '2026-11-10T17:10:00Z',
  ...overrides,
});

/** `GET /teams/{id}/meetings` entries. */
export const meetingSummary = (overrides: Record<string, unknown> = {}) => ({
  id: MEETING_ID,
  teamId: TEAM_ID,
  title: 'Retro #3',
  status: 'active',
  phase: 'reflect',
  facilitator: { id: ALICE.id, displayName: ALICE.displayName, pictureVersion: null },
  participants: [participant(ALICE, { role: 'admin' })],
  taskCount: 0,
  groupCount: 0,
  createdAt: new Date().toISOString(),
  endedAt: null,
  ...overrides,
});
