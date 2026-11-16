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
    // Live once acknowledged: what was read before it is re-read.
    expect(seen.map((e) => e.type)).toEqual(['resync', 'meeting.started']);
    expect(seen[1]!.payload).toEqual({ meetingId: 'm' });
  });

  it('asks for a resync only once a subscription is acknowledged', async () => {
    const rt = client();
    const team: string[] = [];
    const meeting: string[] = [];
    rt.subscribe('team:1', (e) => team.push(e.type));
    await flushPromises();
    const socket = MockWebSocket.last!;
    socket.open();
    rt.subscribe('meeting:9', (e) => meeting.push(e.type));
    expect(socket.sent.map((m) => m.channel)).toEqual(['team:1', 'meeting:9']);
    expect(team).toEqual([]);

    socket.emit({ type: 'subscribed', channel: 'meeting:9' });
    expect(meeting).toEqual(['resync']);
    expect(team).toEqual([]);
    // A repeated acknowledgement asks nothing more.
    socket.emit({ type: 'subscribed', channel: 'meeting:9' });
    expect(meeting).toEqual(['resync']);

    // A refused subscription gets its error, and no resync.
    socket.emit({ type: 'error', channel: 'team:1', payload: { code: 'team_not_found' } });
    socket.emit({ type: 'subscribed', channel: 'team:1' });
    expect(team).toEqual(['error']);
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
    rt.subscribe('team:1', () => undefined);
    await flushPromises();
    MockWebSocket.last!.open();
    MockWebSocket.last!.emit({ type: 'subscribed', channel: 'meeting:9' });
    expect(seen).toEqual(['resync']);
    seen.length = 0;

    MockWebSocket.last!.drop();
    expect(rt.status.value).toBe('reconnecting');
    expect(MockWebSocket.instances).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1000);
    expect(MockWebSocket.instances).toHaveLength(2);
    expect(ticket).toHaveBeenCalledTimes(2);

    const second = MockWebSocket.last!;
    second.open();
    expect(rt.status.value).toBe('open');
    expect(second.channels).toEqual(['meeting:9', 'team:1']);
    // Not before the server confirms: events until then would still be missed.
    expect(seen).toEqual([]);
    second.emit({ type: 'subscribed', channel: 'meeting:9' });
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

  it('is connecting during the first attempt, reconnecting once an attempt failed', async () => {
    const ticket = vi.fn(async () => null as string | null);
    ticket.mockResolvedValueOnce(null).mockResolvedValueOnce('t2');
    const rt = client(ticket as never);
    rt.subscribe('team:1', () => undefined);
    // The first attempt: nothing was lost yet.
    expect(rt.status.value).toBe('connecting');
    await flushPromises();
    // No ticket: the attempt failed, the client keeps trying.
    expect(rt.status.value).toBe('reconnecting');
    await vi.advanceTimersByTimeAsync(1000);
    expect(rt.status.value).toBe('reconnecting');
    MockWebSocket.last!.open();
    expect(rt.status.value).toBe('open');
  });

  it('is reconnecting when the first socket closes before it opened', async () => {
    const rt = client();
    const seen: RealtimeEvent[] = [];
    rt.subscribe('team:1', (e) => seen.push(e));
    await flushPromises();
    expect(rt.status.value).toBe('connecting');
    MockWebSocket.last!.close();
    expect(rt.status.value).toBe('reconnecting');
    await vi.advanceTimersByTimeAsync(1000);
    MockWebSocket.last!.open();
    expect(rt.status.value).toBe('open');
    // Nothing is acknowledged yet, so there is nothing to resync.
    expect(seen).toEqual([]);
  });

  it('sends the writing state once subscribed, and again after a reconnect', async () => {
    const rt = client();
    rt.subscribe('meeting:1', () => undefined);
    // Before the acknowledgement it is only remembered.
    rt.setWriting('meeting:1', { column: 'delta', mode: 'new' });
    await flushPromises();
    const socket = MockWebSocket.last!;
    socket.open();
    socket.emit({ type: 'subscribed', channel: 'meeting:1' });
    expect(socket.sent).toEqual([
      { type: 'subscribe', channel: 'meeting:1' },
      { type: 'writing', channel: 'meeting:1', column: 'delta', mode: 'new' },
    ]);
    // The same state is not sent twice; a change is sent at once.
    rt.setWriting('meeting:1', { column: 'delta', mode: 'new' });
    rt.setWriting('meeting:1', { column: 'plus', mode: 'edit' });
    expect(socket.sent.at(-1)).toEqual({
      type: 'writing',
      channel: 'meeting:1',
      column: 'plus',
      mode: 'edit',
    });
    expect(socket.sent).toHaveLength(3);

    socket.drop();
    await vi.advanceTimersByTimeAsync(2000);
    const again = MockWebSocket.last!;
    expect(again).not.toBe(socket);
    again.open();
    again.emit({ type: 'subscribed', channel: 'meeting:1' });
    expect(again.sent.at(-1)).toEqual({
      type: 'writing',
      channel: 'meeting:1',
      column: 'plus',
      mode: 'edit',
    });

    rt.setWriting('meeting:1', null);
    expect(again.sent.at(-1)).toEqual({ type: 'writing', channel: 'meeting:1', column: null });
  });

  it('holds writing changes past the rate limit, then sends the latest', async () => {
    const rt = client();
    rt.subscribe('meeting:1', () => undefined);
    await flushPromises();
    const socket = MockWebSocket.last!;
    socket.open();
    socket.emit({ type: 'subscribed', channel: 'meeting:1' });
    // Twelve quick toggles that end with the editor closed.
    for (let i = 0; i < 12; i++) {
      rt.setWriting('meeting:1', i % 2 === 0 ? { column: 'plus', mode: 'new' } : null);
    }
    const writes = () => socket.sent.filter((m) => m.type === 'writing');
    expect(writes()).toHaveLength(8);
    await vi.advanceTimersByTimeAsync(1000);
    expect(writes()).toHaveLength(9);
    expect(writes().at(-1)).toEqual({ type: 'writing', channel: 'meeting:1', column: null });
  });

  it('keeps a refused writing message away from the channel listeners', async () => {
    const rt = client();
    const seen: RealtimeEvent[] = [];
    rt.subscribe('meeting:1', (e) => seen.push(e));
    await flushPromises();
    const socket = MockWebSocket.last!;
    socket.open();
    socket.emit({ type: 'subscribed', channel: 'meeting:1' });
    socket.emit({ type: 'error', channel: 'meeting:1', payload: { code: 'not_writable' } });
    socket.emit({ type: 'error', channel: 'meeting:1', payload: { code: 'not_subscribed' } });
    expect(seen.map((e) => e.type)).toEqual(['resync']);
    socket.emit({ type: 'error', channel: 'meeting:1', payload: { code: 'team_not_found' } });
    expect(seen.map((e) => e.type)).toEqual(['resync', 'error']);
  });
});
