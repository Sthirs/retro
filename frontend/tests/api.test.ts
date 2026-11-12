import { describe, expect, it, vi } from 'vitest';

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
