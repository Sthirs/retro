import { describe, expect, it, vi } from 'vitest';

import { CLIENT_ID, isOwnEvent } from '@/api/clientId';
import { apiFetch } from '@/api/fetcher';

describe('apiFetch', () => {
  it('resolves to the parsed JSON body, status and headers', async () => {
    const fetchMock = vi.fn<(input: Request) => Promise<Response>>(
      async () =>
        new Response(JSON.stringify({ ok: true }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
    );
    vi.stubGlobal('fetch', fetchMock);

    const res = await apiFetch<{ data: unknown; status: number; headers: Headers }>(
      '/api/v1/x?y=1',
      { method: 'POST', body: '{}' },
    );

    expect(res.data).toEqual({ ok: true });
    expect(res.status).toBe(200);
    const request = fetchMock.mock.calls[0]![0];
    expect(request.method).toBe('POST');
    expect(request.url).toBe(`${window.location.origin}/api/v1/x?y=1`);
    expect(request.credentials).toBe('same-origin');
  });

  it('leaves data undefined for an empty or non-JSON response', async () => {
    vi.stubGlobal('fetch', async () => new Response(null, { status: 204 }));
    expect((await apiFetch<{ data: unknown }>('/a', {})).data).toBeUndefined();

    vi.stubGlobal(
      'fetch',
      async () =>
        new Response('<html>Bad Gateway</html>', {
          status: 502,
          headers: { 'Content-Type': 'text/html' },
        }),
    );
    const res = await apiFetch<{ data: unknown; status: number }>('/a', {});
    expect(res).toMatchObject({ data: undefined, status: 502 });
  });
});

describe('client id', () => {
  it('is a UUID, the same on every request of the tab', async () => {
    expect(CLIENT_ID).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
    const fetchMock = vi.fn<(input: Request) => Promise<Response>>(
      async () => new Response(null, { status: 204 }),
    );
    vi.stubGlobal('fetch', fetchMock);
    await apiFetch('/api/v1/a', { method: 'GET' });
    await apiFetch('/api/v1/b', { method: 'POST', body: '{}' });
    const sent = fetchMock.mock.calls.map(([request]) => request.headers.get('X-Client-Id'));
    expect(sent).toEqual([CLIENT_ID, CLIENT_ID]);
  });

  it('is sent again when a request is retried after renewing the session', async () => {
    const { setTokenSource } = await import('@/api/fetcher');
    let token = 'old';
    setTokenSource({
      token: () => token,
      renew: async () => {
        token = 'new';
        return true;
      },
    });
    const fetchMock = vi.fn<(input: Request) => Promise<Response>>(async (request) =>
      request.headers.get('Authorization') === 'Bearer new'
        ? new Response(null, { status: 204 })
        : new Response(null, { status: 401 }),
    );
    vi.stubGlobal('fetch', fetchMock);
    await apiFetch('/api/v1/a', { method: 'GET' });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[1]![0].headers.get('X-Client-Id')).toBe(CLIENT_ID);
    setTokenSource(null);
  });

  it("recognises this tab's own events only", () => {
    expect(isOwnEvent({ clientId: CLIENT_ID })).toBe(true);
    expect(isOwnEvent({ clientId: '0190a4e0-0000-7000-8000-0000000000ff' })).toBe(false);
    expect(isOwnEvent({ clientId: null })).toBe(false);
    expect(isOwnEvent({})).toBe(false);
    expect(isOwnEvent(null)).toBe(false);
  });
});
