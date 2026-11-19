# ADR-0015: Named writing presence for tasks in Discuss

- **Status:** 🟡 proposed
- **Date:** 2026-11-19
- **Deciders:** Retro maintainers

## Context

The Tasks panel of Discuss (spec §9.5) shows who is working on a task right now: a placeholder
*"Sara is writing a task…"* while someone has an unsent task for the current group, and
*"Marco is editing…"* on a task whose inline editor someone has open. Anyone in the meeting can
edit or delete any task, so these hints are what keeps two people from rewriting the same task
at once.

[ADR-0012](0012-ephemeral-client-messages-and-user-events.md) gives the hub a socket-bound,
unpersisted `writing` command, aggregated across replicas. It carries counts only, by design:
reflections are anonymous during Reflect. Tasks are not anonymous, since every task shows its
author, so the panel needs the writer's identity and the task they edit. The Group phase's drag
state (`card.dragging`, also ADR-0012's follow-up) already carries a user per card, aggregated the
same way.

## Decision

We will add a second writing command for tasks, which names the user:

- **Client command `taskWriting`.** A client sends
  `{"type":"taskWriting","channel":"meeting:{id}","groupId":uuid|null,"taskId":uuid|null}` on a
  meeting channel it is subscribed to. `groupId` means "writing a new task for this group",
  `taskId` means "editing this task", both `null` means it stopped. One state per socket per
  meeting, which ends with the subscription or the socket, exactly as `writing` does. The server
  accepts it only from a participant, while the meeting runs in Discuss, for the current group or
  for a task written in this meeting; anything else answers `not_writable` and drops the socket's
  state. It shares the 10 messages per second budget of ephemeral client messages.
- **Aggregation like drags.** Each replica keeps one entry (user, target) per socket and announces
  its absolute list per meeting in an internal, unpersisted `task_writing.changed` notification.
  Every replica folds these per replica and sends local subscribers
  `task.writing {writers: [{userId, groupId, taskId}]}` (no `seq`) when the cluster-wide list
  changes. A user writing in two tabs appears once. Presence sync, silent-replica expiry and
  listener reconnects apply unchanged.
- **Snapshot.** `GET /meetings/{id}/state` holds the current `taskWriters` (empty when the meeting
  is not active).
- **Client states.** A tab announces `taskId` while its inline editor is open on a task, otherwise
  `groupId` while it holds a non-empty draft for the current group, otherwise `null`. It sends
  only transitions, as for reflections.

## Alternatives considered

- **Extending `writing` with a task target** — the reflection command and its event are anonymous
  by contract; mixing a named variant into them would make an accidental leak of reflection
  writers one field away.
- **Counts only ("1 task being written")** — cannot say which task is being edited, which is the
  point of the hint now that anyone may edit any task.
- **Locking a task while it is edited** — needs lock expiry and takeover, and blocks a facilitator
  who wants to fix a typo; a visible hint is enough at the size of a retrospective.
- **Persisting the state** — rejected for the same reasons as in ADR-0012.

## Sources / Prior art

- Spec §9.5 (Tasks panel), §10.1 (anyone edits or deletes any task), §11.1–§11.3 (ephemeral
  messages, throttling, `NOTIFY`).
- ADR-0011 (channels, presence across replicas), ADR-0012 (socket-bound writing state, drag
  presence).
- Named typing and "is editing" indicators in collaborative tools: Matrix typing notifications
  (<https://spec.matrix.org/latest/client-server-api/#typing-notifications>), Slack's
  `user_typing`, and presence avatars on cells or blocks in Google Docs and Notion.

## Consequences

- Positive: participants see who is about to add or change a task, live, and indicators clear
  themselves when a tab closes or a replica dies.
- Negative: writers are visible by name, so the command must never be reused for anonymous
  content; the shared ephemeral budget now serves three kinds of message.
- Negative: like drags, the list is eventually consistent across replicas.
- Follow-ups: showing the same hint on the Tasks board while a meeting is active.
