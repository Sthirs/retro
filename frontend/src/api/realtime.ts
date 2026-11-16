/**
 * The real-time WebSocket (spec §11, ADR-0011): one socket per tab, opened
 * with a short-lived ticket, carrying `meeting:{id}` and `team:{id}` channels.
 * It reconnects with exponential backoff and re-subscribes. Once the server
 * acknowledges a subscription (the first one, or after a reconnect) the
 * channel gets a `resync` event: whatever its listeners read over REST before
 * that may have missed events published until the acknowledgement.
 *
 * It also carries the tab's ephemeral writing state (ADR-0012), which the
 * server ties to the socket: it is sent again after every acknowledgement, so
 * a reconnect restores it.
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

/** What the tab is writing in a meeting: a column and whether it is a new reflection. */
export interface WritingState {
  column: string;
  mode: 'new' | 'edit';
}

/**
 * `idle`: no subscriptions; `connecting`: the first attempt is under way;
 * `reconnecting`: the socket dropped, or an attempt failed, and it is coming back.
 */
export type RealtimeStatus = 'idle' | 'connecting' | 'open' | 'reconnecting';

const MAX_BACKOFF_MS = 30_000;
/** Error codes that answer a `writing` message, not a subscription (ADR-0012). */
const WRITING_ERRORS = new Set(['not_subscribed', 'not_writable']);
/** How long the socket stays open without subscriptions (page changes resubscribe quickly). */
const IDLE_CLOSE_MS = 10_000;
/**
 * Writing messages sent per rolling second. The server drops ephemeral
 * messages past 10 per second and socket (ADR-0012); this stays below it,
 * with room for network jitter.
 */
const WRITING_PER_SECOND = 8;
const WRITING_WINDOW_MS = 1000;

export class RealtimeClient {
  readonly status: Ref<RealtimeStatus> = ref('idle');
  private socket: WebSocket | null = null;
  private readonly handlers = new Map<string, Set<RealtimeHandler>>();
  /** Channels whose `subscribe` is sent but not yet acknowledged. */
  private readonly awaitingAck = new Set<string>();
  private attempts = 0;
  private wasOpen = false;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private idleTimer: ReturnType<typeof setTimeout> | null = null;
  private generation = 0;
  /** The attempt (generation) waiting for its ticket, if any. */
  private ticketFor: number | null = null;
  /** Writing state per meeting channel, re-sent after each acknowledgement. */
  private readonly writing = new Map<string, WritingState>();
  /** When the socket's recent writing messages were sent. */
  private writingSentAt: number[] = [];
  /** Channels whose latest writing state waits for the rate budget. */
  private readonly writingHeld = new Set<string>();
  private writingTimer: ReturnType<typeof setTimeout> | null = null;

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
    if (first) this.requestSubscription(channel);
    this.ensureConnected();
    return () => this.unsubscribe(channel, handler);
  }

  /**
   * Tells the meeting `channel` what this tab is writing (`null`: nothing).
   * Sent at once while the subscription is live, otherwise on its ack.
   */
  setWriting(channel: string, state: WritingState | null): void {
    const before = this.writing.get(channel);
    if (before?.column === state?.column && before?.mode === state?.mode) return;
    if (state) this.writing.set(channel, state);
    else this.writing.delete(channel);
    if (this.handlers.has(channel) && !this.awaitingAck.has(channel)) {
      this.sendWriting(channel, state);
    }
  }

  /**
   * Sends the channel's writing state if the rate budget allows; otherwise
   * holds it, and sends the latest state once the budget frees up.
   */
  private sendWriting(channel: string, state: WritingState | null): void {
    const now = Date.now();
    this.writingSentAt = this.writingSentAt.filter((t) => now - t < WRITING_WINDOW_MS);
    if (this.writingSentAt.length >= WRITING_PER_SECOND) {
      this.writingHeld.add(channel);
      if (!this.writingTimer) {
        const wait = this.writingSentAt[0]! + WRITING_WINDOW_MS - now;
        this.writingTimer = setTimeout(() => this.flushWriting(), wait);
      }
      return;
    }
    this.writingHeld.delete(channel);
    this.writingSentAt.push(now);
    this.send({ type: 'writing', channel, column: state?.column ?? null, mode: state?.mode });
  }

  /** Sends the held writing states, as they are now. */
  private flushWriting(): void {
    this.writingTimer = null;
    for (const channel of [...this.writingHeld]) {
      this.writingHeld.delete(channel);
      // A channel waiting for its ack gets its state with the ack.
      if (this.handlers.has(channel) && !this.awaitingAck.has(channel)) {
        this.sendWriting(channel, this.writing.get(channel) ?? null);
      }
    }
  }

  /** Forgets the held writing states and the rate budget (a new socket). */
  private resetWriting(): void {
    if (this.writingTimer) clearTimeout(this.writingTimer);
    this.writingTimer = null;
    this.writingHeld.clear();
    this.writingSentAt = [];
  }

  /** Closes the socket and forgets every subscription (sign-out). */
  disconnect(): void {
    this.handlers.clear();
    this.writing.clear();
    this.close();
  }

  private unsubscribe(channel: string, handler: RealtimeHandler): void {
    const set = this.handlers.get(channel);
    if (!set?.delete(handler) || set.size > 0) return;
    this.handlers.delete(channel);
    this.awaitingAck.delete(channel);
    this.writing.delete(channel);
    this.writingHeld.delete(channel);
    this.send({ type: 'unsubscribe', channel });
    if (this.handlers.size === 0) {
      this.idleTimer = setTimeout(() => this.close(), IDLE_CLOSE_MS);
    }
  }

  private close(): void {
    this.generation++;
    this.clearIdle();
    this.awaitingAck.clear();
    this.resetWriting();
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
    // A ticket is already on its way: that attempt opens the socket.
    if (this.ticketFor === this.generation) return;
    void this.connect();
  }

  private async connect(): Promise<void> {
    const generation = ++this.generation;
    this.ticketFor = generation;
    this.status.value = this.wasOpen || this.attempts > 0 ? 'reconnecting' : 'connecting';
    const ticket = await this.ticket().catch(() => null);
    if (generation !== this.generation) return;
    this.ticketFor = null;
    if (!ticket) {
      this.status.value = 'reconnecting';
      this.scheduleRetry();
      return;
    }
    const socket = new WebSocket(this.url(ticket));
    this.socket = socket;
    socket.onopen = () => {
      if (socket !== this.socket) return;
      this.attempts = 0;
      this.wasOpen = true;
      this.status.value = 'open';
      // A new socket: a fresh rate budget, and every state goes with its ack.
      this.resetWriting();
      for (const channel of this.handlers.keys()) this.requestSubscription(channel);
    };
    socket.onmessage = (message) => {
      if (socket !== this.socket || typeof message.data !== 'string') return;
      let event: RealtimeEvent;
      try {
        event = JSON.parse(message.data) as RealtimeEvent;
      } catch {
        return;
      }
      if (event.type === 'subscribed') {
        // Live from here on: listeners re-read what they may have missed.
        if (this.awaitingAck.delete(event.channel)) {
          const writing = this.writing.get(event.channel);
          if (writing) this.sendWriting(event.channel, writing);
          this.dispatch({ type: 'resync', channel: event.channel, seq: null, payload: {} });
        }
        return;
      }
      if (event.type === 'unsubscribed') return;
      if (event.type === 'error') {
        // A refused writing message: the subscription itself is fine.
        if (WRITING_ERRORS.has(event.payload?.code)) return;
        this.awaitingAck.delete(event.channel);
      }
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

  /** Subscribes on the open socket (else `onopen` does), awaiting the ack. */
  private requestSubscription(channel: string): void {
    this.awaitingAck.add(channel);
    this.send({ type: 'subscribe', channel });
  }

  private send(message: { type: string; channel: string; [key: string]: unknown }): void {
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
