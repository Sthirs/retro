# ADR-0012: Ephemeral client messages and user-targeted events on the real-time hub

- **Status:** 🟡 proposed
- **Date:** 2026-11-16
- **Deciders:** Retro maintainers

## Context

The Reflect phase (spec §9.2) needs two things the hub of
[ADR-0011](0011-realtime-hub.md) does not offer.

- **Writing state.** Everyone sees how many reflections are being written in each column,
  without learning who writes them. A reflection is being written while its author has an
  editor open on it. This state is not a database change: spec §11.1 sends such client
  messages over the WebSocket, unpersisted. ADR-0011 lists "ephemeral client messages
  (typing, drag in progress) and their throttling" as a follow-up.
- **Author-only events.** During Reflect the server must never send a reflection to anyone but
  its author (spec §9.2). The author's other tabs still need to learn about their own changes.
  ADR-0011 has two channel kinds, `meeting:{id}` and `team:{id}`, and every subscriber of a
  channel receives every message on it.

The backend runs as several replicas (spec §2), so writing state must be aggregated across
them, like presence.

## Decision

We will extend the hub as follows:

- **Client command `writing`.** A client sends
  `{"type":"writing","channel":"meeting:{id}","column":"plus"|…|null,"mode":"new"|"edit"}`
  on a meeting channel it is subscribed to while the meeting runs (otherwise an `error`).
  `column: null` means it stopped writing. The state belongs to the socket: one editor per
  socket per meeting, and it ends when the subscription or the socket ends (close,
  unsubscribe, revocation). The server answers nothing on success. Each socket may send at
  most 10 client messages per second; extra `writing` messages are dropped.
- **Aggregation like presence.** Each replica counts its own sockets' writing state per
  meeting, column and mode. When its counts for a meeting change, it publishes an internal,
  unpersisted `writing.changed` notification carrying its replica id and its *absolute*
  counts. Every replica folds these into a map keyed by meeting and replica, and sends local
  subscribers `reflection.writing {counts}` (no `seq`) when the cluster-wide total changes. The
  machinery that keeps presence correct applies unchanged:
  - `presence.sync` re-announces the counts;
  - a replica silent for three heartbeats is forgotten, along with its counts;
  - a listener reconnect restarts from the replica's own sockets.

  The snapshot (`GET /meetings/{id}/state`) includes the current totals.
- **User-targeted notifications.** A notification may name a `user`. Hubs deliver it only to
  that user's sockets on the channel, and strip the field before sending. Such events carry
  no `seq`, since other subscribers would otherwise see gaps. They hold ids only. The
  receiving tab re-reads the state unless the event's `clientId` is its own (the echo
  suppression of ADR-0011). Reflection changes publish `reflection.changed {id, clientId}` to
  the author, and `reflection.count_changed {counts}` with a `seq` to everyone.

## Alternatives considered

- **A `user:{id}` channel** — clients would subscribe to one more channel and the server
  would authorise it. It is also unordered with the meeting channel's events. A target field
  on the existing channel needs neither.
- **Persisting writing state in a table** — a closed tab or killed replica leaves rows behind
  that need expiry, and every editor open or close becomes a database write. Socket-bound
  state goes away by itself.
- **Debounced typing events (one per 2 s while typing)** — clients would have to time
  indicators out, and an editor left open without typing would look idle. The product defines
  "being written" as "an editor is open", which is a state, not a stream of events.
- **Count-only events and a full re-read by every client** — every reflection change would make
  every participant fetch the snapshot.

## Sources / Prior art

- Spec §9.2 (privacy during Reflect, being written), §11.1 (ephemeral client messages), §11.2
  (event catalogue), §11.3 (PostgreSQL backbone, throttling, payload limit).
- ADR-0011 (channels, `seq`, presence across replicas, echo suppression via `X-Client-Id`)
  and its follow-up on ephemeral client messages.
- Typing indicators in chat products are ephemeral and socket-bound, as in Slack's RTM
  `user_typing` and Matrix's typing EDUs, which are not part of the persisted room timeline:
  <https://spec.matrix.org/latest/client-server-api/#typing-notifications>.

## Consequences

- Positive: indicators clear themselves when a tab closes or a replica dies. Author-only data
  never reaches another user's socket.
- Positive: absolute counts make announcements idempotent; a missed or repeated one is
  corrected by the next.
- Negative: writing totals are eventually consistent across replicas (same bounds as
  presence).
- Negative: user-targeted events are unsequenced, so a tab that misses one recovers only on
  its next re-read (reconnect, gap or resubscribe).
- Follow-ups: the same command shape can carry drag-in-progress state for the Group phase;
  per-socket rate limiting of every client message.
