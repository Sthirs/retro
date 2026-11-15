# ADR-0011: Real-time hub over WebSocket with PostgreSQL LISTEN/NOTIFY fan-out

- **Status:** 🟡 proposed
- **Date:** 2026-11-15
- **Deciders:** Retro maintainers

## Context

Meetings are live: every participant must see the current person, phase, facilitator and
presence change as it happens, and teammates must see a new meeting appear on their
Meetings page (spec §8, §9.1, §11). The task board must update live too (spec §10.1). The
spec fixes the transport (one WebSocket per client at `/api/v1/ws`, channels `meeting:{id}`
and `team:{id}`, JSON messages with a per-channel `seq`) and the backbone (PostgreSQL
`LISTEN/NOTIFY`, no Valkey, spec §11.3). Access tokens live in memory and are sent as
`Authorization: Bearer` (ADR-0005), which browsers cannot set on a WebSocket handshake, and
tokens must never appear in logged URLs (spec §3.3). The backend runs as several stateless
replicas behind one ingress (spec §2, §17).

## Decision

We will build the hub as follows:

- **Authentication.** `POST /api/v1/ws-ticket` (Bearer-authenticated) returns a ticket valid
  for 30 seconds. The ticket is a stateless HS256 JWT (user id, expiry) signed with the JWT
  secret, so any replica can verify it; its issuer differs from access tokens', so neither is
  accepted in place of the other. The client opens
  `/api/v1/ws?ticket=…`; the ticket is short-lived, so leaking it in an access log is harmless.
- **Protocol.** After the handshake the client sends `{"type":"subscribe","channel":"…"}` or
  `{"type":"unsubscribe","channel":"…"}`. The server checks team membership for each
  subscription (a meeting channel resolves to its team) and answers
  `{"type":"subscribed","channel":"…"}` or
  `{"type":"error","channel":"…","seq":null,"payload":{"code":"…"}}`.
  Events are `{"type","channel","seq","payload"}`. The server pings every 25 seconds.
- **Revocation.** Membership is checked at subscribe time, so it must also be withdrawn live.
  Removing a member publishes `member.removed` and deleting a team publishes `team.deleted` on
  the team channel, in the same transaction. Every replica's hub turns these into a revocation
  `(team, user | all)` on an in-process broadcast that every socket listens to from the moment
  it opens (so a subscription authorised concurrently is still revoked). A socket ends its
  subscriptions to that team's channels, including its meetings', leaves their presence, and
  sends an `error` with `team_not_found` on each. A socket that lags on the
  revocation broadcast re-checks every subscription against the database.
- **Fan-out.** State-changing requests call `pg_notify('retro_events', …)` inside the same
  transaction as the write, so an event exists only if the write committed. Each replica keeps
  one dedicated `sqlx::PgListener` connection (reconnecting with backoff) and forwards each
  notification to an in-process `tokio::sync::broadcast` sender per channel, which feeds its
  local sockets.
- **Ordering.** `seq` is per channel: `meetings.event_seq` and `teams.event_seq` are
  incremented in the writing transaction, under the row lock that already serialises writes to
  that meeting or team. A client that sees a gap, or reconnects, re-reads the full state
  (`GET /meetings/{id}/state`, or the list it shows) instead of replaying events. No event log
  is kept. A write that touches a meeting locks the team row first and the meeting row second
  (the order of every team mutation, including the cascade of a team delete), so meeting
  writes, task writes and team deletion cannot deadlock.
- **Payloads.** Notifications carry ids and the changed fields only, well below the
  8000-byte `NOTIFY` limit. Clients fetch bulky data over REST.
- **Presence.** Each replica counts its own sockets per `(meeting, user)` and publishes
  ephemeral `presence.changed` notifications (no `seq`, never persisted) when a user's first
  socket subscribes or last socket leaves. Every replica folds these into a cluster-wide map
  keyed by replica id. A starting replica publishes `presence.sync`, to which every replica
  answers by re-announcing its local sockets. A replica holds one async lock from a socket
  count transition until its notification is sent, so its online/offline announcements commit
  (and reach every replica) in the order of the transitions. Presence changes go to the
  meeting channel and, as `presence.changed` with the meeting id, to the team channel, where
  clients patch the meeting card instead of reloading the list.
- **Echo suppression.** Each tab sends a random `X-Client-Id` header with its REST requests;
  task events carry it as `clientId`, so the tab that made a change does not refetch for its
  own echo while its other tabs still do.
- **Abstraction.** Business code only calls `ws::publish*(txn, …)`, and sockets only call
  `Hub::subscribe`; both live in `backend/src/ws/`, so a different broker can replace PostgreSQL
  there without touching business code (spec §11.3). A `PubSub` trait is extracted when a
  second implementation exists.

## Alternatives considered

- **Server-Sent Events** — one-way only, so typing and drag events (spec §9.2, §9.3) would
  need a second channel; the spec already names WebSocket.
- **Bearer token in the WebSocket URL or a subprotocol header** — the token would be logged
  by proxies, or would need a long-lived credential on the socket; spec §3.3 forbids it.
- **Valkey / Redis pub/sub** — an extra stateful dependency, and no delivery-on-commit
  guarantee; spec §11.3 rejects it at the target scale.
- **Persisted event log for catch-up** — needs retention and cleanup; a state re-read is
  simpler and always correct.
- **A global sequence for `seq`** — values taken in concurrent transactions can commit out of
  order; a per-row counter under the existing lock cannot.

## Sources / Prior art

- Spec §3.3 (WebSocket ticket), §8 (meetings), §10.1 (live board), §11.1–11.3 (transport,
  events, PostgreSQL backbone), §16.2 (`ws/` module), §20 (latency and scale targets).
- ADR-0003 (Axum, SeaORM) and ADR-0005 (Bearer access tokens), whose follow-ups name the hub
  and ticketed WebSocket authentication.
- PostgreSQL `NOTIFY` semantics, delivery on commit and the payload limit:
  <https://www.postgresql.org/docs/current/sql-notify.html>.
- sqlx `PgListener`: <https://docs.rs/sqlx/latest/sqlx/postgres/struct.PgListener.html>.
- Axum WebSocket extractor: <https://docs.rs/axum/latest/axum/extract/ws/index.html>.
- Ticket-based WebSocket authentication, as used by Slack's RTM `rtm.connect`, and the
  WHATWG WebSocket API's lack of custom headers:
  <https://websockets.spec.whatwg.org/#the-websocket-interface>.

## Consequences

- Positive: events are consistent with the database (no event for a rolled-back write) and
  reach every replica without a new dependency.
- Positive: clients recover from any missed event by re-reading state, so the hub stays
  stateless apart from presence.
- Negative: each replica holds one extra direct PostgreSQL connection, which must bypass a
  transaction-mode pooler.
- Negative: presence is eventually consistent across replicas and is lost (then re-announced)
  when a replica restarts.
- Follow-ups: ephemeral client messages (typing, drag in progress) and their throttling;
  exporting `pg_notification_queue_usage()` as a metric; per-socket rate limiting.
