import { flushPromises } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { RealtimeClient, type RealtimeEvent } from '@/api/realtime';

import { MockWebSocket } from './helpers';

describe('RealtimeClient', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
    vi.stubGlobal('WebSocket', MockWebSocket);
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  const client = (ticket = vi.fn(async () => 't1')) =>
    new RealtimeClient(ticket, (t) => `ws://test/api/v1/ws?ticket=${t}`);

  it('opens one socket with a ticket and subscribes each channel once', async () => {
    const rt = client();
    const seen: RealtimeEvent[] = [];
    rt.subscribe('team:1', (e) => seen.push(e));
    rt.subscribe('team:1', () => undefined);
    await flushPromises();
    expect(MockWebSocket.instances).toHaveLength(1);
    const socket = MockWebSocket.last!;
    expect(socket.url).toBe('ws://test/api/v1/ws?ticket=t1');
    expect(rt.status.value).toBe('connecting');

    socket.open();
    expect(rt.status.value).toBe('open');
    expect(socket.sent).toEqual([{ type: 'subscribe', channel: 'team:1' }]);

    socket.emit({ type: 'subscribed', channel: 'team:1' });
    socket.emit({
      type: 'meeting.started',
      channel: 'team:1',
      seq: 1,
      payload: { meetingId: 'm' },
    });
    socket.emit({ type: 'meeting.started', channel: 'team:2', seq: 1, payload: {} });
    expect(seen.map((e) => e.type)).toEqual(['meeting.started']);
    expect(seen[0]!.payload).toEqual({ meetingId: 'm' });
  });

  it('unsubscribes when the last listener leaves, and closes when idle', async () => {
    const rt = client();
    const stop = rt.subscribe('team:1', () => undefined);
    await flushPromises();
    const socket = MockWebSocket.last!;
    socket.open();
    stop();
    expect(socket.channels).toEqual([]);
    vi.advanceTimersByTime(10_000);
    expect(socket.readyState).toBe(MockWebSocket.CLOSED);
    expect(rt.status.value).toBe('idle');
  });

  it('reconnects with backoff, re-subscribes and asks listeners to resync', async () => {
    const ticket = vi.fn(async () => 't1');
    const rt = client(ticket);
    const seen: string[] = [];
    rt.subscribe('meeting:9', (e) => seen.push(e.type));
    await flushPromises();
    MockWebSocket.last!.open();

    MockWebSocket.last!.drop();
    expect(rt.status.value).toBe('reconnecting');
    expect(MockWebSocket.instances).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1000);
    expect(MockWebSocket.instances).toHaveLength(2);
    expect(ticket).toHaveBeenCalledTimes(2);

    const second = MockWebSocket.last!;
    second.open();
    expect(rt.status.value).toBe('open');
    expect(second.channels).toEqual(['meeting:9']);
    expect(seen).toEqual(['resync']);
  });

  it('retries when no ticket can be had', async () => {
    const ticket = vi.fn(async () => null as string | null);
    ticket.mockResolvedValueOnce(null).mockResolvedValueOnce('t2');
    const rt = client(ticket as never);
    rt.subscribe('team:1', () => undefined);
    await flushPromises();
    expect(MockWebSocket.instances).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(1000);
    expect(MockWebSocket.last?.url).toContain('ticket=t2');
  });

  it('forgets everything on disconnect', async () => {
    const rt = client();
    rt.subscribe('team:1', () => undefined);
    await flushPromises();
    const socket = MockWebSocket.last!;
    socket.open();
    rt.disconnect();
    expect(socket.readyState).toBe(MockWebSocket.CLOSED);
    expect(rt.status.value).toBe('idle');
    await vi.advanceTimersByTimeAsync(60_000);
    expect(MockWebSocket.instances).toHaveLength(1);
  });
});
