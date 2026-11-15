/**
 * The real-time WebSocket (spec §11, ADR-0011): one socket per tab, opened
 * with a short-lived ticket, carrying `meeting:{id}` and `team:{id}` channels.
 * It reconnects with exponential backoff and re-subscribes; after a
 * reconnect every channel gets a `resync` event, since events may have been
 * missed in between.
 */
import { ref, type Ref } from 'vue';

import { api } from './client';

/** A message on a channel. `seq` is `null` for ephemeral events (presence, resync). */
export interface RealtimeEvent {
  type: string;
  channel: string;
  seq: number | null;
  // Event payloads are defined by the backend per event type (spec §11.2).
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  payload: any;
}

export type RealtimeHandler = (event: RealtimeEvent) => void;

/**
 * `idle`: no subscriptions; `connecting`: the first attempt is under way;
 * `reconnecting`: the socket dropped, or an attempt failed, and it is coming back.
 */
export type RealtimeStatus = 'idle' | 'connecting' | 'open' | 'reconnecting';

const MAX_BACKOFF_MS = 30_000;
/** How long the socket stays open without subscriptions (page changes resubscribe quickly). */
const IDLE_CLOSE_MS = 10_000;

export class RealtimeClient {
  readonly status: Ref<RealtimeStatus> = ref('idle');
  private socket: WebSocket | null = null;
  private readonly handlers = new Map<string, Set<RealtimeHandler>>();
  private attempts = 0;
  private wasOpen = false;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private idleTimer: ReturnType<typeof setTimeout> | null = null;
  private generation = 0;

  constructor(
    private readonly ticket: () => Promise<string | null> = fetchTicket,
    private readonly url: (ticket: string) => string = socketUrl,
  ) {}

  /** Listens to a channel; returns the unsubscribe function. */
  subscribe(channel: string, handler: RealtimeHandler): () => void {
    let set = this.handlers.get(channel);
    const first = !set;
    if (!set) {
      set = new Set();
      this.handlers.set(channel, set);
    }
    set.add(handler);
    this.clearIdle();
    if (first) this.send({ type: 'subscribe', channel });
    this.ensureConnected();
    return () => this.unsubscribe(channel, handler);
  }

  /** Closes the socket and forgets every subscription (sign-out). */
  disconnect(): void {
    this.handlers.clear();
    this.close();
  }

  private unsubscribe(channel: string, handler: RealtimeHandler): void {
    const set = this.handlers.get(channel);
    if (!set?.delete(handler) || set.size > 0) return;
    this.handlers.delete(channel);
    this.send({ type: 'unsubscribe', channel });
    if (this.handlers.size === 0) {
      this.idleTimer = setTimeout(() => this.close(), IDLE_CLOSE_MS);
    }
  }

  private close(): void {
    this.generation++;
    this.clearIdle();
    if (this.retryTimer) clearTimeout(this.retryTimer);
    this.retryTimer = null;
    this.socket?.close();
    this.socket = null;
    this.attempts = 0;
    this.wasOpen = false;
    this.status.value = 'idle';
  }

  private clearIdle(): void {
    if (this.idleTimer) clearTimeout(this.idleTimer);
    this.idleTimer = null;
  }

  private ensureConnected(): void {
    if (this.socket || this.retryTimer || this.handlers.size === 0) return;
    void this.connect();
  }

  private async connect(): Promise<void> {
    const generation = ++this.generation;
    this.status.value = this.wasOpen || this.attempts > 0 ? 'reconnecting' : 'connecting';
    const ticket = await this.ticket().catch(() => null);
    if (generation !== this.generation) return;
    if (!ticket) {
      this.status.value = 'reconnecting';
      this.scheduleRetry();
      return;
    }
    const socket = new WebSocket(this.url(ticket));
    this.socket = socket;
    socket.onopen = () => {
      if (socket !== this.socket) return;
      const reconnected = this.wasOpen;
      this.attempts = 0;
      this.wasOpen = true;
      this.status.value = 'open';
      for (const channel of this.handlers.keys()) this.send({ type: 'subscribe', channel });
      if (reconnected) {
        for (const channel of this.handlers.keys()) {
          this.dispatch({ type: 'resync', channel, seq: null, payload: {} });
        }
      }
    };
    socket.onmessage = (message) => {
      if (socket !== this.socket || typeof message.data !== 'string') return;
      let event: RealtimeEvent;
      try {
        event = JSON.parse(message.data) as RealtimeEvent;
      } catch {
        return;
      }
      if (event.type === 'subscribed' || event.type === 'unsubscribed') return;
      this.dispatch({ ...event, seq: event.seq ?? null, payload: event.payload ?? {} });
    };
    socket.onclose = () => {
      if (socket !== this.socket) return;
      this.socket = null;
      if (this.handlers.size === 0) {
        this.status.value = 'idle';
        return;
      }
      this.status.value = 'reconnecting';
      this.scheduleRetry();
    };
  }

  private scheduleRetry(): void {
    const delay = Math.min(MAX_BACKOFF_MS, 1000 * 2 ** this.attempts);
    this.attempts++;
    // Jitter, so a restarted backend is not hit by every client at once.
    const jittered = delay / 2 + Math.random() * (delay / 2);
    this.retryTimer = setTimeout(() => {
      this.retryTimer = null;
      this.ensureConnected();
    }, jittered);
  }

  private send(message: { type: string; channel: string }): void {
    if (this.socket?.readyState === WebSocket.OPEN) this.socket.send(JSON.stringify(message));
  }

  private dispatch(event: RealtimeEvent): void {
    for (const handler of [...(this.handlers.get(event.channel) ?? [])]) handler(event);
  }
}

async function fetchTicket(): Promise<string | null> {
  const res = await api.wsTicket();
  return res.status === 200 ? res.data.ticket : null;
}

function socketUrl(ticket: string): string {
  const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
  return `${scheme}://${window.location.host}/api/v1/ws?ticket=${encodeURIComponent(ticket)}`;
}

/** The app's socket. */
export const realtime = new RealtimeClient();
