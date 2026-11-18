# Retro — Specifications

Retro is a web application for running team retrospectives. It has a **Vue 3 + TypeScript** frontend (SPA) and a **Rust** backend that exposes an HTTP + WebSocket API. Data is stored in **PostgreSQL**. The application is packaged as Docker images and deployed to Kubernetes with **Helm**.

---

## Table of contents

1. [Glossary](#1-glossary)
2. [Architecture overview](#2-architecture-overview)
3. [Authentication](#3-authentication)
4. [Users](#4-users)
5. [Teams](#5-teams)
6. [Invitations & join links](#6-invitations--join-links)
7. [Application layout & navigation](#7-application-layout--navigation)
8. [Meetings](#8-meetings)
9. [Retrospective phases](#9-retrospective-phases)
10. [Tasks](#10-tasks)
11. [Real-time events](#11-real-time-events)
12. [Permissions matrix](#12-permissions-matrix)
13. [Data model](#13-data-model)
14. [REST API](#14-rest-api)
15. [Frontend](#15-frontend)
16. [Backend](#16-backend)
17. [Deployment](#17-deployment)
18. [Development environment](#18-development-environment)
19. [Continuous integration](#19-continuous-integration)
20. [Non-functional requirements](#20-non-functional-requirements)

---

## 1. Glossary

| Term | Definition |
| --- | --- |
| **User** | A person authenticated through the OIDC provider. |
| **Team** | A group of users that hold meetings together and share a task board. |
| **Member** | A user that belongs to a team. |
| **Team admin** | A member with administrative rights on a team. Every team has at least one. |
| **Meeting** | A retrospective session held by a team. It goes through five phases. |
| **Facilitator** | The member who leads a meeting. Initially, the creator of the meeting. |
| **Phase** | One of the steps of a retrospective: Icebreaker, Reflect, Group, Vote, Discuss. |
| **Reflection** | An anonymous card written during the Reflect phase, in one of four categories. |
| **Group** | A named stack of one or more reflections, created during the Group phase. |
| **Task** | An action item created during the Discuss phase and tracked on the team's Tasks board. |

---

## 2. Architecture overview

```text
┌──────────────┐   HTTPS (REST + WebSocket)   ┌──────────────┐     SQL     ┌──────────────┐
│   Frontend   │ ───────────────────────────▶ │   Backend    │ ──────────▶ │  PostgreSQL  │
│ Vue 3 + TS   │                              │    Rust      │             │ (external in │
│ (static SPA) │ ◀─────── realtime events ─── │  API + WS    │             │  production) │
└──────┬───────┘                              └──────┬───────┘             └──────────────┘
       │ browser redirect (sign-in page)             │ OIDC Authorization Code + PKCE
       └──────────────────▶ OIDC Provider ◀──────────┘ (confidential client)
```

- **Frontend**: a static single-page application served by its own container (e.g. nginx).
- **Backend**: a stateless Rust service exposing a REST API under `/api/v1` and a WebSocket endpoint for real-time meeting and board updates. It owns the database schema.
- **Database**: PostgreSQL is the **only** stateful dependency. It stores all application data, including uploaded images, and carries real-time fan-out between backend replicas (`LISTEN/NOTIFY`, see §11.3). It is provisioned separately in production and can be deployed inside Minikube in development. No cache or message broker (e.g. Valkey/Redis) is required.
- **Identity provider** (optional): any OIDC-compliant provider (e.g. Keycloak, Auth0, Entra ID, Google) for single sign-on. Retro also has its own username/password accounts (§3).

---

## 3. Authentication

Authentication is performed by the **backend**, which issues Retro's own session for both sign-in methods (ADR-0005).

### 3.1 Sign-in methods

- **Username and password** (local accounts). The sign-in form accepts a username or an email address (case-insensitive). Passwords are stored as Argon2id hashes. Wrong credentials and unknown accounts produce the same error and take the same time. Failed attempts are rate-limited per login name and per client IP. Local sign-in can be disabled (`AUTH_LOCAL_LOGIN_ENABLED`).
- **Single sign-on** through an OIDC provider, optional (`OIDC_ENABLED`). The backend is a **confidential client** using the Authorization Code flow with **PKCE** (S256), `state` and `nonce`. It validates the ID token (signature via the provider's JWKS, cached and refreshed on key rotation; `iss`, `aud`, `exp`, `nonce`) and requires `email_verified`. Provider tokens never reach the browser.
- The sign-in page shows "Forgot password?" only when a password reset URL is configured (`AUTH_PASSWORD_RESET_URL`); the application sends no email (§6.1).

### 3.2 Accounts and SSO identities

- An SSO identity is keyed by the provider's `iss` + `sub`. On SSO sign-in, in one transaction:
  1. a known identity signs in its user;
  2. otherwise, a user with the same (verified) email gets the identity linked;
  3. otherwise, a new user is created when auto-registration is on (`OIDC_AUTO_REGISTER`, default on), or the sign-in is refused.
- `email` is synced from the provider's claims on each SSO sign-in. The display name is synced too, until the user sets their own on their account page. The `picture` claim is not displayed: provider-hosted images are blocked by the Content Security Policy.

### 3.3 Session

- After sign-in the backend returns a short-lived **access token** (JWT signed by Retro, default 15 minutes) and sets a **refresh token** in an `httpOnly`, `SameSite=Strict` cookie scoped to `/api/v1/auth`.
- The frontend keeps the access token in memory (not `localStorage`) and sends `Authorization: Bearer <access_token>` on each API request. On a `401` it renews once through `POST /auth/refresh` and retries. After a reload, the session is restored from the refresh cookie.
- Refresh tokens are opaque, single-use and rotated on every refresh; only their hash is stored. Presenting an already-rotated token revokes the whole session (token family), except within a few seconds, to tolerate several tabs refreshing at once.
- **"Keep me signed in"** makes the refresh cookie persistent (30 days by default). Otherwise it is a browser-session cookie and the session expires after 12 hours at most.
- After single sign-on, the backend sets the refresh cookie and redirects to `/auth/callback`; **no token is ever put in a URL**.
- The WebSocket connection authenticates with a short-lived ticket obtained from `POST /api/v1/ws-ticket` with the access token. Tokens are never put in URLs that get logged.
- **Logout** revokes the session (refresh token family) and clears the cookie. Signing out of the identity provider (`end_session_endpoint`) is a later addition.
- Configuration (secrets, token lifetimes, OIDC issuer, client ID and secret, scopes `openid email profile`, button text) is supplied through environment variables / Helm values. The redirect URI to register with the provider is `<public URL>/api/v1/auth/oidc/callback`.

---

## 4. Users

- A user has: id, email (unique), optional username (unique, local accounts), display name, avatar URL (the provider's picture claim, stored but not displayed), optional password hash, created/updated timestamps, and zero or more linked SSO identities (§3.2).
- A user can belong to zero or more teams.
- A user with no teams sees an empty state that invites them to create a team or accept an invitation.
- The user's email is the key used to match pending invitations (see §6).
- **Your account** page (`/account`, from the user menu): display name (editable, trimmed, 1–80 characters), picture (upload PNG, JPEG, GIF or WebP up to 5 MB, or remove; initials otherwise), email (read-only), sign-in method, member since, the user's teams (open, leave, create), pending invitations (accept, decline), preferences (theme, language, team opened by default on this browser) and log out.
- A user's picture is visible to themselves and to members of teams they share.

---

## 5. Teams

### 5.1 Rules

- A team has a **name**, a set of **members**, and at least **one team admin** at all times.
- The user who creates a team becomes its first admin.
- **Team admins** can:
  - rename the team;
  - promote members to admin and demote admins (the last admin cannot be demoted);
  - remove members from the team (the last admin cannot be removed).
- **Members** can:
  - invite other users (by email or join link, see §6);
  - leave the team. The last admin cannot leave until another admin is promoted. If the last admin is also the only member, leaving deletes the team (after confirmation).
- Only members can see a team, its meetings and its tasks.
- **Deleting a team** (admin only, with confirmation) permanently deletes **everything related to it**: memberships, invitations, join link, meetings, reflections, groups, votes, reactions, tasks and uploaded images. This is enforced with `ON DELETE CASCADE` foreign keys.

### 5.2 Team settings page

- Team name (editable by admins).
- Member list with roles. Admins see promote/demote/remove actions.
- Pending invitations, which can be revoked.
- Join link management (see §6.2).

---

## 6. Invitations & join links

### 6.1 Invitations by email address

- Any member can invite a user by entering their email address.
- The backend creates an `Invitation` (team, email, inviter, token, expiry — default 7 days). **The application does not send emails.**
- A team has at most one open invitation per email address. Inviting that address again gives the open invitation back to its inviter and to admins; other members get an "already invited" error.
- The invitation appears in the invitee's **pending invitations** (user menu and onboarding screen) when they log in with an account whose email matches. They can accept or decline it there. Accepting makes them a member.
- After creating an invitation, the inviter can copy its personal accept link (`/invite/{token}`) and share it through any channel. Only the inviter and admins can see that link; other members see the pending invitation without it. Opening the link takes the user through login (if needed) and then the acceptance step. The token in the link is the credential: whoever holds the link can accept it, whatever the email of their account. The acceptance step shows the email the invitation was sent to.
- Invitations can be revoked by the inviter or by an admin. Accepting a revoked or expired invitation fails with a clear message.
- When a user becomes a member by any route (invitation or join link), the open invitations to that team for their email are closed as accepted. A user who is already a member and opens someone else's invitation link does not use it up: accepting or declining it leaves it open.
- When a member leaves or is removed, the open invitations whose link they could see (all of them for an admin, their own for a member) get a new token, so a former member cannot rejoin with a link they knew. The invitations stay open: the invitee can still accept them from their pending invitations, and the inviter and admins can copy the new link.

### 6.2 Join link

- Each team can have a **join link** (a random, unguessable token).
- Any authenticated user who opens the link joins the team as a member.
- Any member can copy the join link. Admins can regenerate it (which invalidates the old one) or disable it.
- The join link is regenerated automatically whenever a member leaves or is removed, so a former member cannot rejoin with the link they knew.

### 6.3 Meeting invite link

- While a meeting is **active**, it has an **invite link** (`/m/{inviteToken}`). Any participant can copy it from the **Invite** button in the meeting header.
- Opening the link takes the user through login if needed. Then:
  - if the user is **not a member** of the team, they are added to the team as a **member**, as with the team join link;
  - the user joins the meeting as a participant and lands directly in the meeting room.
- The link stops working when the meeting ends. The facilitator can regenerate it, which invalidates the previous one.
- The meeting invite link works even if the team join link (§6.2) is disabled. It is scoped to the running meeting.
- When a member leaves or is removed, the invite links of the team's active meetings are regenerated, so a former member cannot rejoin the team with a meeting link they knew. Links already shared stop working; participants copy the new one from the **Invite** button.

---

## 7. Application layout & navigation

### 7.1 Top bar

- **Team selector** (dropdown) listing the user's teams. Its last entry is **"+ Add team"**, which opens a dialog to create a team.
- The selected team is the **context of the whole UI**: meetings, tasks and settings all refer to it. Changing team re-routes to the same section of the new team.
- The selected team is part of the URL (`/teams/:teamId/...`) and the last used team is remembered per user (stored in the browser).
- **Default team**: in *Your account* the user picks the team Retro opens after sign-in (and at `/`). Either a specific team, or **Last used** (the default), which opens the last used team. The choice is stored per user in the browser; a pinned team the user has left falls back to the last used one.
- User menu on the right: avatar, name, pending invitations, theme, **Your account**, logout.

### 7.2 Left menu

| Entry | Behavior |
| --- | --- |
| **Add meeting** (button) | Starts a new retrospective for the selected team and navigates into it. The creator becomes its facilitator. |
| **Meetings** | Lists the team's **active** meetings. Each entry shows creation time, facilitator, current phase and participants, with a **Join** button. Below it, a list of **ended** meetings opens each one's summary (§8.3). |
| **Tasks** | The team's task board (see §10). |
| **Team settings** | Team management (see §5.2). |

---

## 8. Meetings

### 8.1 Lifecycle

- A meeting belongs to a team and is created by a member, who becomes its **facilitator**.
- State: `active` → `ended`. Several active meetings can exist for a team at the same time.
- Phases run in this fixed order: **Icebreaker → Reflect → Group → Vote → Discuss**. Phases advance only forward and only by the facilitator.
- Any team member can join an active meeting. Joining makes them a **participant**. Presence (online/offline) is tracked through the WebSocket connection.
- There is exactly one **facilitator**. Only the facilitator can move to the next person, group or phase.
- The facilitator can hand the role to another participant at any time.
- A **team admin** can **take over** the facilitator role at any time. All participants are notified of the change.
- At any time the facilitator can **End meeting**. A confirmation dialog is shown first. Ending:
  - sets the state to `ended` and records `ended_at`;
  - invalidates the meeting invite link (§6.3);
  - notifies all participants, who are redirected to the meeting summary (§8.3);
  - makes the meeting read-only;
  - publishes the tasks created in it on the team's Tasks board (tasks are persisted immediately when created, so the board updates live).

### 8.3 Meeting summary (ended meetings)

Team members can open any ended meeting, which shows a read-only **summary** with:

- date, duration and facilitator;
- **participants**;
- **groups**, sorted by votes, each with its name, **vote total** and its anonymous reflections;
- **tasks** created during the meeting, with their **current** board status and a link to the Tasks board.

The individual meeting phases cannot be replayed.

### 8.2 Common meeting UI

- **Title**: each meeting has a title, **"Retro #N"** by default (`N` counts the team's meetings, starting at 1). The facilitator can rename it from the pencil next to the title (1–80 characters) while the meeting is active.
- Header: meeting title, team name and date, current phase with a stepper showing all five phases, facilitator name, participant avatars with presence, and an **Invite** button that copies the meeting invite link (§6.3).
- Facilitator controls: **Next** (person/group/phase depending on context), **Timer**, **Settings** (e.g. votes), **Transfer facilitator**, **End meeting**.
- Team admins who are not the facilitator see a **Take over facilitation** action.
- **Ready** button at the bottom for participants (used in Reflect, Group and Vote; refused in the other phases). The facilitator sees a counter such as *"4 / 6 ready"* (out of all participants) and who is ready; the others see who is facilitating. Ready states reset when the phase changes.
- **Timer** (facilitator), in any phase:
  - set a duration (presets of 3, 5, 10 or 15 minutes, or a custom `mm:ss` from 1 second to 99:59) and start it;
  - pause and resume it, add one minute (also after it ran out, which restarts the countdown from now), and stop it.

  All participants see the same countdown, which is computed from server time. The timer carries over when the phase changes, until the facilitator stops it. When it reaches zero, everyone gets a non-blocking, dismissible notice (*"Time's up. Giulia decides when to move on."*); it never advances the phase.

---

## 9. Retrospective phases

### 9.1 Icebreaker

- At meeting creation the backend picks:
  - a **greeting** ("Hello") in a randomly chosen language, shown with its pronunciation (IPA) and the language name (e.g. *"Ciao! /ˈtʃa.o/ — Italian"*);
  - an **icebreaker question** from a curated list.

  Both differ from meeting to meeting. Where possible, the backend avoids repeating the team's recent picks.
- The participants are shown **one at a time** on everyone's screen (avatar + name), in a randomized order. The meeting starts in this phase, so the order is built as people join: while the facilitator is still on the first person, each newcomer takes a random place among those who have not spoken yet; this shuffles everyone who joins at the start.
- Only the **facilitator** can move to the **next person**. After the last person, the facilitator can move to the **next phase** (Reflect).
- The facilitator can go back to the **previous person**, or to **anyone who has already had their turn** by selecting them in the speaking order. The turn continues from that person: everyone after them is up again, and Next walks through them in the same order.
- Participants who join after the first person has finished are added to the end of the queue.

### 9.2 Reflect

- Four columns: **Plus**, **Delta**, **Ideas**, **Thanks**.
- Each participant writes **reflections**. Each reflection belongs to exactly one column.
- Reflection content is **rich text**: bold, italic, strikethrough, links, bullet lists, inline code, and images (static or animated, e.g. GIFs). Images are pasted, dropped or picked from a file, uploaded to the backend, converted to WebP, stored in PostgreSQL (§13), and referenced by attachment ID (at most 10 per reflection). An image uploaded for a reflection is visible only to its uploader until the reflections are revealed (Group). Content is stored as sanitized HTML (or editor JSON) and **sanitized on the server** against an allow-list.
- Lifecycle of a reflection:
  1. **draft** — being written, only on the author's screen;
  2. **confirmed** — sent by the author.

  A draft never leaves the author's browser: it is kept there (so it survives a reload) and reaches the server only when sent. Once sent, a reflection **cannot be un-sent** (returned to draft). Until the Group phase starts, the author can still **edit** it, **move** it to another column, or **delete** it. After that, reflections are immutable.
- Reflections are written only during the Reflect phase, by participants of the meeting.
- **Privacy during Reflect**:
  - each participant sees only their own reflections in full;
  - for other participants' reflections, everyone sees **only the count per column** (e.g. *"4 from team"*) and how many are **being written** in each column (*"1 being written"*). A reflection is being written while its author has an editor open on it: a new one, or a sent one they are editing;
  - the server never sends another user's reflection content to a client during this phase.
- **Anonymity**: reflections are anonymous to everyone, including the facilitator, in every phase. The author ID is stored only so that authors can edit their own reflections. The API never exposes it to other users.
- The facilitator can set a **timer** (§8.2).
- Participants press **Ready** when done. The facilitator sees the ready count.
- When the facilitator moves to Group, drafts are discarded. The bottom bar warns authors who still have unsent drafts (*"1 unsent draft"*).

### 9.3 Group

- All confirmed reflections are revealed to everyone, **anonymously**. They start in their original columns, each reflection being an ungrouped card, in the order they were sent.
- **Drag & drop**:
  - dropping a card onto another card creates a **group** containing both;
  - dropping a card onto a group adds it to the group;
  - dragging a card out of a group removes it from the group. A group left with a single card dissolves back into a plain card;
  - a group can contain reflections from **different columns**. Each card keeps its column color/label.
- **Placement**: a group sits in the column, and at the place, of the card it was created on. A card leaving a group goes back to its own column and place. Cards and groups are not reordered, and a whole group cannot be dragged or merged into another.
- Only **participants** can group cards and rename groups, only during Group while the meeting is active.
- Each group has a **name** (0–80 characters, trimmed), editable by any participant; the last saved name wins. An empty name is shown as "Untitled group".
- Clicking a group **expands** it to show all its cards. Collapsed, it shows the name, the card count and a stacked preview. Expanding is local to each viewer.
- **Keyboard**: every card has a **Move to…** menu listing the existing groups, the other ungrouped cards to start a new group with, and "Remove from group" for a grouped card.
- **Real-time**: every participant sees moves live, including **who** is dragging which card: the card stays in its place for the others, lifted, with the dragger's avatar badge ("Aisha is moving"). Moves are not anonymous; only reflection authorship is.
- Concurrency: the server is authoritative. Each card carries a version, raised by every move, and conflicting moves are resolved last-write-wins, with the result broadcast to all clients: a card dropped onto a card that has meanwhile joined a group joins that group. A card being dragged by one user is soft-locked for the others (they cannot pick it up); the server does not enforce the lock.
- Participants press **Ready**. The facilitator advances to Vote.
- For voting purposes, an ungrouped card is treated as a group of one.

### 9.4 Vote

- Every participant gets a **vote budget** (default **5**) and can give at most **3 votes** (default) to the same group.
- The facilitator can change both values in the meeting **Settings** (budget 1–20, at most 1–10 per group), until Vote ends. Lowering the budget below what a user has already spent removes that user's most recent votes so they fit the new limit; lowering the maximum per group removes each user's most recent votes on any group above it. The settings dialog warns how many people (never who) would lose votes.
- Each group shows **+** and **−** buttons and the user's own vote count on it. **−** takes back the user's most recent vote on that group. A counter shows remaining votes ("2 of 5 votes left").
- A card never grouped shows as the plain card it was (its implicit single-card group has no name). Clicking a group expands it to read its cards; expanding is local to each viewer.
- During this phase, users see **only their own votes**. Totals are hidden until Discuss, to avoid bias. The facilitator sees how many votes have been cast overall, as one number: nothing per group or per person.
- Participants press **Ready**. The facilitator advances to Discuss.

### 9.5 Discuss

- **Left panel**: the groups, **sorted by total votes** (descending). Ties are broken by group size, then by name. Each entry shows its vote total.
- **Center**: **one group at a time** — the group title and all its reflections (rich text, anonymous, with column label).
- **Reactions**: participants can add emoji reactions to each reflection. Multiple different reactions per reflection are allowed, and a user can add several different emojis to the same reflection (one of each kind). Counts are shown per emoji and reacting is a toggle.
- **Right panel — Tasks**: participants create tasks linked to the current group. Each task has rich text (same editor as reflections, including pasted images) with **@mentions** of team members, plus its author and creation time. New tasks start in the **Active** column of the board.
- **Navigation**: the facilitator moves to the **next** or **previous** group. Everyone follows the facilitator's current group. A participant can preview other groups locally without affecting the others.
- After the last group, the facilitator is prompted to **End meeting**.

---

## 10. Tasks

### 10.1 Board

- Each team has one task board, with four columns: **Future**, **Stuck**, **Active**, **Done**.
- Tasks are created **only during meetings** (Discuss phase).
- Task card:
  - creation date (and source meeting, as a link);
  - rich text (including images), which can contain **@mentions** of team members (rendered as chips; mentioned users can filter "my tasks"). Mentions do **not** trigger notifications;
  - author at the bottom (avatar + name).
- Cards can be **dragged and dropped** between columns and reordered within a column. Any team member can move cards.
- Changes are broadcast in real time to anyone viewing the board (§11).
- Any team member can edit or delete any task.
- Filters: mentioned users (several can be picked; a task matches if it mentions any of them), author, meeting. Filters combine with AND.

### 10.2 Ordering

- Each task has `status` and a `position`. Positions are fractional-index string keys (base 62, compared byte-wise), so that a move updates a single row. A move whose neighbours are out of date (`afterId` not before `beforeId`) conflicts and the client reloads; otherwise the card lands directly after `afterId`, or directly before `beforeId`, whatever other tasks (for example ones hidden by a filter) sit in between.

---

## 11. Real-time events

### 11.1 Transport

- One WebSocket per client at `/api/v1/ws`. Clients **subscribe** to channels:
  - `meeting:{id}` — everything in a meeting;
  - `team:{id}` — team-level changes (meeting started/ended, task board updates, membership changes).
- The server checks team membership before accepting any subscription. When a member leaves or is removed, or the team is deleted, the server ends that user's subscriptions to the team's channels (the team channel and its meetings' channels) at once, with an `error` message on each; a former member receives nothing more.
- Messages are JSON: `{ "type": string, "channel": string, "seq": number, "payload": object }`. Errors have the same shape: `{ "type": "error", "channel", "seq": null, "payload": { "code" } }`.
- Every message carries a per-channel `seq`. After a reconnect, the client either resyncs by fetching the full state (`GET /meetings/{id}/state`) or requests the messages after the last `seq` it saw.
- Client → server **ephemeral** messages (writing state, drag in progress, cursor) go over the WebSocket and are **not persisted**. A user's writing state is tied to their socket, so it ends when the socket closes ([ADR-0012](adr/0012-ephemeral-client-messages-and-user-events.md)). **State-changing** actions go through the REST API (or WS commands), are validated, persisted, then broadcast.
- Protocol details (ticket, subscribe messages, `seq`, presence across replicas): [ADR-0011](adr/0011-realtime-hub.md).
- Horizontal scaling: when more than one backend replica runs, broadcasts go through **PostgreSQL `LISTEN/NOTIFY`** so that every replica reaches its connected clients (see §11.3).

### 11.2 Event catalogue (non-exhaustive)

| Event | Payload highlights | Visible to |
| --- | --- | --- |
| `participant.joined` / `participant.left` | user | all |
| `presence.changed` | user, online (on the team channel also the meeting) | meeting + team channel |
| `facilitator.changed` | new facilitator | all |
| `phase.changed` | phase (clients re-read the state; at Group start this reveals the reflections) | all |
| `timer.updated` | timer (status, endsAt or remainingMs, durationMs; `null` when stopped), serverNow | all |
| `ready.changed` | userId, ready, readyCount | all |
| `icebreaker.person_changed` | index | all |
| `reflection.writing` | per column, how many new and edited reflections are being written (no author; ephemeral, no `seq`) | all |
| `reflection.count_changed` | sent reflections per column | all |
| `reflection.changed` | reflection id, `clientId` (no content; the author's other tabs re-read the state) | author only (no `seq`) |
| `card.dragging` | every card being dragged and who drags it (ephemeral, no `seq`) | all |
| `group.created` / `group.updated` / `group.deleted` | group (id, column, position, name; id only when deleted), the cards whose group changed (id, group id, version), `clientId` | all |
| `vote.settings_changed` | budget, maxPerGroup, `clientId` | all |
| `vote.changed` | `clientId` only (no `seq`; the receiving tab re-reads the state, ADR-0012) | the voter and the facilitator (who re-reads the cast count); also every user whose votes a settings change removed |
| `vote.totals_revealed` | totals per group | all (Discuss start) |
| `discuss.group_changed` | group id | all |
| `reaction.changed` | reflection, emoji, count | all |
| `task.created` / `task.updated` / `task.moved` / `task.deleted` | task, `clientId` of the tab that made the change (from its `X-Client-Id` header, else `null`), so that tab skips its own echo | meeting + team channel |
| `meeting.renamed` | title | meeting + team channel |
| `meeting.started` / `meeting.ended` | meeting | team channel (`meeting.ended` also on the meeting channel) |
| `meeting.updated` | meeting id (phase, facilitator or participants changed) | team channel |
| `member.removed` | user | team channel |
| `team.deleted` | — | team channel |

### 11.3 Real-time backbone: PostgreSQL vs Valkey

**Decision: PostgreSQL alone is enough. Valkey is not needed.**

What the real-time layer has to do:

| Need | How it is handled with PostgreSQL only |
| --- | --- |
| Fan-out of events between backend replicas | `LISTEN/NOTIFY`. Each replica holds **one** dedicated listener connection and forwards events to its local WebSocket subscribers through in-process Tokio broadcast channels. |
| Consistency between persisted state and events | `NOTIFY` issued inside the same transaction as the write is delivered **only on commit**, so clients never see events for rolled-back changes. A dedicated broker can't give this guarantee for free. |
| Ephemeral events (writing, drag in progress, presence) | Throttled on the client (drag state sent only when a card is picked up or dropped; writing state sent only when an editor opens or closes) and on the server (10 messages / s per socket), and sent through `NOTIFY` without being persisted. Each replica announces its own writing counts and dragged cards per meeting, like presence. |
| Timer | Stored as `ends_at` on the meeting. Clients count down locally. No ticking server process is needed. |
| Presence | Tracked in memory by the replica holding the socket and broadcast on change. A replica restart causes clients to reconnect and re-announce. |
| Catch-up after reconnect | Full-state snapshot (`GET /meetings/{id}/state`). No event log retention is needed. |
| Rate limiting | In memory per replica, which is acceptable at the target scale. |

Why this is sufficient at the target scale (§20: 500 concurrent meetings × 50 participants):

- With throttling, the expected peak is a few hundred `NOTIFY`/s cluster-wide. PostgreSQL handles thousands per second.
- Fan-out to the individual sockets happens **in the replicas**, not in PostgreSQL: one notification per event per replica, not one per client.

Constraints and mitigations:

- **8000-byte `NOTIFY` payload limit**: payloads that could exceed it (e.g. the reflections revealed at Group start, rich-text content) are not sent inline. The notification carries only the event type and entity IDs, and each replica loads the data from the database before pushing it to its clients.
- **Global notification queue** (8 GB by default): monitored through `pg_notification_queue_usage()`, which is exported as a Prometheus metric. Listener connections must never block.
- **Connection poolers**: the listener connection must be a direct session connection. Transaction-mode PgBouncer does not support `LISTEN`.

**Extensibility**: the backend abstracts fan-out behind a `PubSub` trait (`publish(channel, event)`, `subscribe(channel)`). If metrics later show `NOTIFY` pressure, a Valkey pub/sub implementation can be added without touching business logic. Until then, PostgreSQL remains the only dependency.

---

## 12. Permissions matrix

| Action | Non-member | Member | Team admin | Facilitator |
| --- | :-: | :-: | :-: | :-: |
| View team / meetings / tasks | ✗ | ✓ | ✓ | ✓ |
| Rename team | ✗ | ✗ | ✓ | — |
| Invite (email / join link) | ✗ | ✓ | ✓ | — |
| Regenerate/disable join link | ✗ | ✗ | ✓ | — |
| Remove member / change roles | ✗ | ✗ | ✓ | — |
| Start meeting | ✗ | ✓ | ✓ | — |
| Join meeting | ✗ | ✓ | ✓ | ✓ |
| Join team + meeting via meeting invite link | ✓ | ✓ | ✓ | ✓ |
| Copy meeting invite link | ✗ | ✓ | ✓ | ✓ |
| Regenerate meeting invite link | ✗ | ✗ | ✗* | ✓ |
| Advance person / phase / group | ✗ | ✗ | ✗* | ✓ |
| Set timer, vote settings | ✗ | ✗ | ✗* | ✓ |
| Rename meeting | ✗ | ✗ | ✗* | ✓ |
| Transfer facilitator role | ✗ | ✗ | ✗* | ✓ |
| Take over facilitator role | ✗ | ✗ | ✓ | — |
| End meeting | ✗ | ✗ | ✗* | ✓ |
| View ended meeting summary | ✗ | ✓ | ✓ | ✓ |
| Write/edit own reflections (until Group) | ✗ | ✓ | ✓ | ✓ |
| Group cards, rename groups | ✗ | ✓ | ✓ | ✓ |
| Vote, react, create tasks | ✗ | ✓ | ✓ | ✓ |
| Move tasks on board | ✗ | ✓ | ✓ | ✓ |
| Edit/delete task | ✗ | ✓ | ✓ | ✓ |

\* Team admins cannot perform facilitator actions directly. They first **take over the facilitator role**, which they can do at any time, and then act as the facilitator.

---

## 13. Data model

The backend owns the schema and manages it through ORM migrations. All IDs are UUIDv7 and all timestamps are `timestamptz` (UTC).

```text
users(id, email UNIQUE, username UNIQUE NULL, display_name, display_name_custom BOOL DEFAULT false,
      avatar_url NULL, -- provider picture claim; not displayed (see user_pictures)
      picture_version NULL, password_hash NULL, created_at, updated_at)
    -- email and username are stored lower-cased

user_pictures(user_id PK → users, content_type, size_bytes, sha256, data BYTEA, created_at)

user_identities(id, user_id → users, issuer, subject, created_at)
    UNIQUE(issuer, subject)

refresh_tokens(id, user_id → users, family_id, token_hash UNIQUE, persistent,
               expires_at, revoked_at NULL, replaced_by NULL, created_at)

teams(id, name, join_token NULL, join_enabled, created_by → users, created_at, updated_at,
      event_seq)                                   -- last `seq` sent on `team:{id}` (§11)

team_members(team_id → teams, user_id → users, role ENUM('admin','member'), joined_at)
    PK(team_id, user_id)

invitations(id, team_id → teams, email, token UNIQUE, invited_by → users,
            status ENUM('pending','accepted','declined','revoked','expired'), expires_at, created_at)
    UNIQUE(team_id, email) WHERE status = 'pending'   -- one open invitation per email

meetings(id, team_id → teams, created_by → users, facilitator_id → users,
         number,                                   -- 1, 2, … per team; UNIQUE(team_id, number)
         title,                                    -- "Retro #N" by default, renamable
         status ENUM('active','ended'),
         invite_token UNIQUE,                      -- meeting invite link, invalid once ended
         phase ENUM('icebreaker','reflect','group','vote','discuss'),
         greeting_text, greeting_language, icebreaker_question,
         greeting_lang_code,                       -- BCP 47, for the `lang` attribute
         greeting_pronunciation,                   -- IPA, e.g. "/ˈtʃa.o/"
         icebreaker_order UUID[], icebreaker_index,
         vote_budget DEFAULT 5, vote_max_per_group DEFAULT 3,
         discuss_group_id NULL,
         timer_state JSONB NULL,
         event_seq,                                -- last `seq` sent on `meeting:{id}` (§11)
         created_at, ended_at NULL)

meeting_participants(meeting_id, user_id, joined_at, is_ready)
    PK(meeting_id, user_id)

reflections(id, meeting_id, author_id → users,        -- never exposed to other users
            column ENUM('plus','delta','ideas','thanks'),
            content,                                 -- sent reflections only; drafts stay in the browser
            group_id NULL → reflection_groups,
            position INT,                            -- place in its column, set when Group starts
            version INT,                             -- raised by every move between groups
            created_at, updated_at)

reflection_groups(id, meeting_id → meetings,
                  column ENUM('plus','delta','ideas','thanks'), position INT,  -- of the card it was created on
                  name,                              -- 0–80 characters, '' = "Untitled group"
                  created_at, updated_at)

votes(id, meeting_id → meetings, user_id → users, group_id → reflection_groups,
      created_at)                                 -- one row per vote, so "most recent" is exact
    -- budget and maximum per group enforced in the application, under the meeting lock
    -- votes on single ungrouped cards: a group of one is created implicitly at Vote start

reactions(reflection_id, user_id, emoji, created_at)
    PK(reflection_id, user_id, emoji)

tasks(id, team_id, meeting_id NULL, group_id NULL, author_id → users,
      content, status ENUM('future','stuck','active','done'), position TEXT,
      created_at, updated_at)
    UNIQUE(team_id, status, position)

task_mentions(task_id, user_id)  PK(task_id, user_id)

attachments(id, team_id → teams, uploader_id → users,  -- uploader never exposed
            content_type,                            -- always 'image/webp'
            animated, width, height, size_bytes, sha256, data BYTEA, created_at)

reflection_attachments(reflection_id, attachment_id)  PK(reflection_id, attachment_id)
task_attachments(task_id, attachment_id)              PK(task_id, attachment_id)

icebreaker_questions(id, text, active)
greetings(id, language, lang_code, text, pronunciation)
```

Notes:

- When Vote starts, every ungrouped card is wrapped in an implicit single-card group, so votes always reference groups.
- **Images** pasted into reflections or tasks are stored **only in PostgreSQL** (`attachments.data BYTEA`). There is no object storage. Uploads are limited to 5 MB and to the image types PNG, JPEG, GIF and WebP (detected from the bytes, never from the declared type; SVG is refused).
  - Every upload is **fully decoded and re-encoded as WebP** ([ADR-0013](adr/0013-image-attachments.md)): animated GIF and WebP stay animated, JPEG and lossy WebP become lossy WebP, PNG, GIF and lossless WebP become lossless WebP. Only pixels survive: metadata (EXIF, GPS, ICC, XMP, comments) and any trailing bytes are dropped, and a JPEG's EXIF orientation is applied first. Limits: 4096 px per side, 16.7 megapixels per frame, 500 frames, 100 megapixels over all frames, 10 MB after conversion.
  - Rich text references an image as `<img data-attachment-id="…" alt="…" width="…" height="…">`, never by URL. The server derives `reflection_attachments` / `task_attachments` from it on save (at most 10 images per content). A saved item may reference only attachments of its own team that it already referenced or that the saving user uploaded; any other reference is rejected with 400 `attachment_not_found`.
  - The backend serves them through `GET /attachments/{id}` to team members who uploaded the image or can see an item referencing it: a task, or a reflection whose meeting has left the Reflect phase. Successful responses are `image/webp` with long-lived `Cache-Control: private, immutable` headers.
  - A user can upload at most 60 images per hour and hold at most 50 attachments not referenced by any reflection or task. Unreferenced attachments older than 24 hours are removed by a periodic cleanup job.
- **Cascade deletion**: every team-scoped table references `teams` (directly or through `meetings`) with `ON DELETE CASCADE`, so deleting a team removes all of its data (§5.1).
- Seed data (50 greetings, each "Hello" in a different language with its pronunciation, and 50 icebreaker questions) is loaded by a migration. A new meeting takes one of each at random.

---

## 14. REST API

Base path: `/api/v1`. JSON throughout. Errors use [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) problem details. An OpenAPI 3 document is generated from the code and served at `/api/v1/openapi.json`. The frontend generates its TypeScript client and types from this document.

### 14.1 Users & auth

| Method | Path | Description |
| --- | --- | --- |
| GET | `/auth/config` | Sign-in options: local sign-in on/off, SSO on/off and button text, password reset URL |
| POST | `/auth/login` | `{ login, password, remember }` → access token + user; sets the refresh cookie |
| POST | `/auth/refresh` | Rotates the refresh cookie → new access token + user |
| POST | `/auth/logout` | Revokes the session and clears the cookie |
| GET | `/auth/oidc?redirect=&remember=` | Starts single sign-on (redirect to the provider) |
| GET | `/auth/oidc/callback` | Provider callback; sets the refresh cookie, redirects to `/auth/callback` (or `/login?error=…`) |
| GET | `/me` | Current user, their teams and pending invitations |
| PATCH | `/me` | `{ displayName }` |
| PUT | `/me/picture` | Raw image body |
| DELETE | `/me/picture` | Remove the picture |
| GET | `/users/{id}/picture` | Picture; owner or teammates |
| POST | `/ws-ticket` | Short-lived ticket to open the WebSocket |

### 14.2 Teams

| Method | Path | Description |
| --- | --- | --- |
| GET | `/teams` | Teams of the current user |
| POST | `/teams` | Create a team (creator becomes admin) |
| GET | `/teams/{teamId}` | Team details |
| PATCH | `/teams/{teamId}` | Rename (admin) |
| DELETE | `/teams/{teamId}` | Delete (admin) |
| GET | `/teams/{teamId}/members` | List members |
| PATCH | `/teams/{teamId}/members/{userId}` | Change role (admin) |
| DELETE | `/teams/{teamId}/members/{userId}` | Remove (admin) or leave (self) |
| POST | `/teams/{teamId}/invitations` | Invite by email |
| GET | `/teams/{teamId}/invitations` | List pending invitations |
| DELETE | `/teams/{teamId}/invitations/{id}` | Revoke |
| GET | `/invitations/{token}` | Preview an invitation for the holder of its link: team, inviter, email, expiry, status |
| POST | `/invitations/{token}/accept` | Accept an invitation |
| POST | `/invitations/{token}/decline` | Decline an invitation |
| GET | `/teams/{teamId}/join-link` | Get the join link |
| POST | `/teams/{teamId}/join-link` | Regenerate / enable / disable (admin) |
| POST | `/join/{joinToken}` | Join through the link |

### 14.3 Meetings

| Method | Path | Description |
| --- | --- | --- |
| GET | `/teams/{teamId}/meetings?status=active\|ended` | List meetings |
| POST | `/teams/{teamId}/meetings` | Start a meeting |
| GET | `/meetings/{id}/state` | Full snapshot, filtered by the caller's permissions and the current phase |
| PATCH | `/meetings/{id}` | `{ title }`: rename (facilitator) |
| GET | `/meetings/{id}/summary` | Summary of an ended meeting (§8.3) |
| POST | `/meetings/{id}/join` | Join as participant |
| GET | `/meetings/{id}/invite-link` | Get the meeting invite link |
| POST | `/meetings/{id}/invite-link` | Regenerate the meeting invite link (facilitator) |
| POST | `/meeting-invites/{inviteToken}/accept` | Join the team (if needed) and the meeting |
| POST | `/meetings/{id}/facilitator` | Transfer (facilitator) or take over (team admin) the facilitator role |
| POST | `/meetings/{id}/phase/next` | Advance phase (facilitator) |
| POST | `/meetings/{id}/icebreaker/next` | Next person (facilitator) |
| POST | `/meetings/{id}/icebreaker/goto` | `{ userId }`: back to someone who already had their turn (facilitator) |
| PUT | `/meetings/{id}/timer` | Start / pause / stop the timer (facilitator) |
| PUT | `/meetings/{id}/ready` | Set own ready flag |
| PATCH | `/meetings/{id}/settings` | `{ voteBudget, voteMaxPerGroup }`: vote budget / max per group (facilitator, until Vote ends; 400 `invalid_vote_settings` out of bounds, 409 `wrong_phase` after Vote) |
| POST | `/meetings/{id}/end` | End the meeting (facilitator) |

### 14.4 Reflections, groups, votes, reactions

| Method | Path | Description |
| --- | --- | --- |
| POST | `/meetings/{id}/reflections` | Create a draft/confirmed reflection |
| PATCH | `/reflections/{rid}` | Edit content/column/status (author, before Group) |
| DELETE | `/reflections/{rid}` | Delete (author, before Group) |
| POST | `/teams/{teamId}/attachments` | Upload an image (raw body, PNG, JPEG, GIF or WebP, ≤ 5 MB) for a reflection or task; answers 201 `{ id, contentType, width, height, animated }`. Errors: 400 `invalid_image` / `image_dimensions_too_large`, 413 `image_too_large`, 415 `unsupported_image_type`, 429 `too_many_requests` / `too_many_pending_attachments` |
| GET | `/attachments/{aid}` | Download an image as WebP (members who can see it, §13) |
| POST | `/meetings/{id}/groups` | `{ targetId, reflectionId }`: drop a card onto another card, creating a group of both (or joining the target's group if it has one) |
| PATCH | `/groups/{gid}` | `{ name }`: rename a group |
| PUT | `/reflections/{rid}/group` | `{ groupId \| null }`: move a card into a group, or out of its group |
| PUT | `/groups/{gid}/vote` | `{ "delta": +1 \| -1 }` |
| PUT | `/reflections/{rid}/reactions/{emoji}` | Add a reaction |
| DELETE | `/reflections/{rid}/reactions/{emoji}` | Remove a reaction |
| PUT | `/meetings/{id}/discuss/current` | Set the current group (facilitator) |

`PUT /groups/{gid}/vote` is open to participants during Vote and answers with the meeting state, holding the caller's own votes (`myVotes`) and, for the facilitator only, every voter's anonymous usage (`voteUsage`: votes spent and the most on one group). Errors: 400 `invalid_vote` (delta neither +1 nor −1), 403 `not_participant`, 404 `group_not_found`, 409 `meeting_ended`, `wrong_phase`, `vote_budget_exhausted`, `vote_group_limit`, `no_vote_to_remove`.

The group endpoints are open to participants during Group (409 `wrong_phase`, `meeting_ended`, 403 `not_participant`) and answer with the meeting state. Unknown or foreign ids give 404 `reflection_not_found` / `group_not_found`; a card dropped onto itself gives 400 `same_reflection`, and a name over 80 characters 400 `invalid_group_name`.

### 14.5 Tasks

| Method | Path | Description |
| --- | --- | --- |
| GET | `/teams/{teamId}/tasks` | Board, with filters |
| POST | `/meetings/{id}/tasks` | Create a task (Discuss phase) |
| PATCH | `/tasks/{tid}` | Edit content |
| PUT | `/tasks/{tid}/position` | `{ status, beforeId?, afterId? }`: the moved task ends up directly after `afterId`, or directly before `beforeId` when only that is given; with neither, at the end of the column. A stale pair (`afterId` not before `beforeId`) answers 409 `stale_position` |
| DELETE | `/tasks/{tid}` | Delete |

### 14.6 Operational

| Method | Path | Description |
| --- | --- | --- |
| GET | `/healthz` | Liveness (served at the root, not under `/api/v1`) |
| GET | `/readyz` | Readiness (database reachable, migrations applied; served at the root) |
| GET | `/metrics` | Prometheus metrics (internal port) |

---

## 15. Frontend

### 15.1 Stack

- **Vue 3** (Composition API, `<script setup>`), **TypeScript** (strict), **Vite**.
- **Vue Router** for routing and **Pinia** for state.
- **Tiptap** (ProseMirror) as the rich text editor, with extensions for bold, italic, link, image paste and mention.
- **DOMPurify** for client-side defensive sanitization when rendering rich text.
- Drag and drop: **vue-draggable-plus** (SortableJS) or `@vueuse/integrations`, used for both the Group phase and the Tasks board.
- API client generated from the backend's OpenAPI document with **orval** (`fetch` client) (ADR-0002).
- No UI component library: components are built from the design tokens as CSS variables (light and dark themes) with scoped styles (ADR-0002). Plus Jakarta Sans is self-hosted.

### 15.2 Routes

```text
/login                          → sign-in page (username/password, single sign-on)
/auth/callback                  → landing page after single sign-on
/invite/:token                  → accept invitation
/join/:joinToken                → join team via link
/m/:inviteToken                 → join team (if needed) + running meeting
/teams/:teamId/meetings         → active + past meetings
/teams/:teamId/meetings/:id     → meeting room (phase-driven view), or summary if ended
/teams/:teamId/tasks            → task board
/teams/:teamId/settings         → team settings
/account                        → your account
/                               → redirect to the last used team, or onboarding
```

### 15.3 Structure

```text
frontend/
  src/
    api/          generated client + ws client
    auth/         OIDC setup, guards
    components/   shared UI (TeamSelector, Avatar, RichTextEditor, Timer, ReadyBar…)
    features/
      teams/
      meetings/
        phases/   Icebreaker.vue, Reflect.vue, Group.vue, Vote.vue, Discuss.vue
      tasks/
    stores/       Pinia stores (session, team, meeting, tasks)
    router/
  tests/
    unit/         Vitest
    e2e/          Playwright
  Dockerfile
```

### 15.4 Behavior

- The meeting store applies WebSocket events to a local snapshot. Actions are applied optimistically and reconciled with server responses.
- The app reconnects automatically with exponential backoff and shows a "Reconnecting…" banner.
- Images are protected by authentication, so `<img src>` cannot fetch them directly. The frontend loads them with an authenticated `fetch`, displays them as `blob:` URLs (custom Tiptap image node view), and caches them per session.
- The frontend is served from the same origin as the API (the ingress routes `/api` to the backend) and reads sign-in options from `GET /api/v1/auth/config`, so a single image works in every environment without runtime configuration files.

---

## 16. Backend

### 16.1 Stack

- **Rust** (stable, edition 2024), **Tokio** runtime.
- **Axum** for HTTP and WebSocket.
- **SeaORM** as the ORM, with **sea-orm-migration** for schema migrations. The backend owns the schema: migrations run automatically at startup (guarded by a PostgreSQL advisory lock) or as a Helm pre-upgrade Job.
- **openidconnect** (OIDC confidential client, ID token validation with JWKS), **jsonwebtoken** (Retro's access tokens), **argon2** (password hashes), **axum-extra** cookies.
- **ammonia** for server-side HTML sanitization.
- **utoipa** for OpenAPI generation.
- **tracing** + `tracing-subscriber` (JSON logs), Prometheus metrics.
- Configuration from environment variables, validated at startup.

### 16.2 Structure

```text
backend/
  src/
    main.rs
    config.rs
    auth/          token validation, extractor for current user
    api/           routers & handlers per resource
    ws/            connection hub, channels, PubSub trait + PostgreSQL LISTEN/NOTIFY impl
    domain/        business rules (phase transitions, vote limits, permissions)
    entities/      SeaORM entities
    services/      attachments (PostgreSQL BYTEA), cleanup jobs
  migration/       sea-orm-migration crate (+ seed data)
  tests/           integration tests
  Dockerfile
```

### 16.3 Rules enforced on the server

- Membership and role checks on every endpoint and every WS subscription.
- Phase guards: e.g. no reflection edits after Reflect, votes only during Vote, tasks only during Discuss.
- Vote limits (budget and maximum per group) checked transactionally.
- Reflection content is never leaked to other users during Reflect, and `author_id` is never serialized for other users.
- At least one admin per team, enforced transactionally.
- Only the facilitator can advance person, group or phase. Team admins must take over the role first.
- Confirmed reflections cannot return to draft.
- Meeting invite tokens are accepted only while the meeting is active.

---

## 17. Deployment

### 17.1 Container images

- `frontend/Dockerfile`: multi-stage — Node build stage, then an `nginx` (unprivileged) runtime serving the static build, with SPA fallback and the runtime `config.json` entrypoint.
- `backend/Dockerfile`: multi-stage — `rust` build stage with dependency caching (e.g. `cargo-chef`), then a minimal runtime (`distroless/cc` or `debian-slim`), running as a non-root user.
- Images are tagged with the git SHA and semver releases.

### 17.2 Helm chart

The application is deployed to Kubernetes with a Helm chart (`deploy/helm/retro`).

- **Frontend** and **backend** are deployed as separate microservices, each with its own `Deployment`, `Service`, resource limits, probes and `HorizontalPodAutoscaler` (optional).
- An **Ingress** (optional, configurable class/TLS) routes `/` to the frontend and `/api` to the backend, with WebSocket upgrade enabled.
- **ConfigMap**: non-secret configuration (OIDC issuer/client ID, public URLs, log level, vote defaults).
- **Secrets**: database password and auth secrets (JWT secret, SSO state secret, OIDC client secret). Alternatively, `existingSecret`s can be referenced.
- **Migration Job** (optional, one per release revision) running the backend's migration command; new pods become ready once it has applied their migrations (ADR-0004).
- The **database is not deployed** by the chart in production. The chart assumes PostgreSQL is already available and reads connection details from `values.yaml`. Development-only PostgreSQL and dex templates render only with `devMode.enabled` (ADR-0006).

Example `values.yaml` excerpt:

```yaml
frontend:
  image: { repository: ghcr.io/<org>/retro-frontend, tag: "" }
  replicas: 2
backend:
  image: { repository: ghcr.io/<org>/retro-backend, tag: "" }
  replicas: 2
  runMigrations: job   # job | startup | none
auth:
  localLogin: true
  existingSecret: retro-auth   # JWT_SECRET, STATE_SECRET, OIDC_CLIENT_SECRET
oidc:
  enabled: true
  issuerUrl: https://idp.example.com/realms/retro
  clientId: retro
database:
  host: postgres.example.com
  port: 5432
  name: retro
  user: retro
  password: ""          # or use existingSecret
  sslMode: require
  existingSecret: ""
ingress:
  enabled: true
  className: nginx
  host: retro.example.com
  tls: true
```

### 17.3 Production

- PostgreSQL is **provisioned separately** (managed service or a dedicated cluster) and is **not part of the application deployment**.
- TLS terminates at the ingress. Pods run as non-root with a read-only root filesystem.

---

## 18. Development environment

The full guide is [`docs/development.md`](development.md) (ADR-0007).

- **Minikube** (ingress addon) provides a local Kubernetes cluster. The same Helm chart deploys the application into it, with the `values-dev.yaml` overlay. The application is served at `retro.<minikube ip>.nip.io`.
- In development, **PostgreSQL** (ephemeral) and **dex** (OIDC provider, served under `/dex` on the application host) run inside Minikube, rendered by the chart only in dev mode.
- **Seed users**: in dev mode the backend seeds at least two users (`alice`, `bob`, password `password`) at startup. dex has the same users, so each can sign in with a password or through single sign-on. dex also has `carol@retro.local`, who exists only there and can sign in only through single sign-on.
- **Demo data**: in dev mode the backend also creates demo teams with tasks for `alice` and `bob` at startup, only when they are missing, so a fresh cluster has populated boards. Further password-only users exist for the end-to-end suite, which never changes `alice`'s and `bob`'s data.
- **Telepresence** lets you run the frontend and/or backend **locally** (with hot reload: `vite dev`, `cargo watch`) while they talk to the services in the cluster as if they were running there.
- **Helper scripts** (`scripts/`):

| Script | Purpose |
| --- | --- |
| `scripts/setup.sh` | Check tooling (Node, Rust, Docker/Podman, Minikube, Helm, kubectl, Telepresence) and install project dependencies |
| `scripts/dev.sh` | One command: start the cluster, build images, deploy, restart, print the URL and users |
| `scripts/cluster-up.sh` | Start Minikube and enable the ingress addon |
| `scripts/build.sh` | Build the frontend/backend images inside Minikube |
| `scripts/deploy.sh` | `helm upgrade --install` with `values-dev.yaml` |
| `scripts/restart.sh` | Restart the app pods after rebuilding the `:dev` images |
| `scripts/dev-backend.sh` / `scripts/dev-frontend.sh` | Run a service locally (with a Telepresence intercept) |
| `scripts/telepresence-*.sh` | Connect, intercept, leave |
| `scripts/db-reset.sh` | Reset the dev database; the backend migrates and re-seeds |
| `scripts/cluster-down.sh` | Tear down |
| `scripts/test-playwright.sh` | Run the end-to-end tests against a deployed instance |

---

## 19. Continuous integration

GitHub Actions runs the pipeline on **every push to `main`** and on **every pull request**. Jobs run in parallel where possible, and the dependency caches (npm, cargo) are kept between runs (ADR-0009).

| Stage | Frontend | Backend |
| --- | --- | --- |
| **Lint** | ESLint (+ `vue-tsc` type check, Prettier check) | `cargo fmt --check`, `cargo clippy -- -D warnings` |
| **Test** | **Vitest** unit tests; **Playwright** end-to-end tests against the app deployed in Minikube with the dev overlay (PostgreSQL and dex) | `cargo test` unit tests; integration tests of the HTTP/WS API against PostgreSQL (testcontainers) |
| **Build** | Docker build of `frontend/Dockerfile` | Docker build of `backend/Dockerfile` |

Additional checks: `helm lint` and `helm template` on the chart, and a check that the generated OpenAPI client is up to date.

Images and the Helm chart are pushed to the registry when a release is published (Release Please). PR builds are build-only. Renovate keeps dependencies up to date.

---

## 20. Non-functional requirements

- **Real-time latency**: events delivered to participants within 300 ms (p95) under normal load.
- **Scale target**: meetings of up to 50 participants, 500 concurrent meetings per deployment.
- **Security**: OWASP ASVS L1 baseline; server-side sanitization of rich text; CSP headers on the frontend; rate limiting on sign-in, invitation and join endpoints; every `/api/v1` response carries `Cache-Control: no-store`, since responses hold tokens (access, refresh, invitation and join links), except successful image responses (`GET /users/{id}/picture` and `GET /attachments/{id}`), which are served `private, immutable` and keyed by version, and carry `X-Content-Type-Options: nosniff` (their error responses stay `no-store`); attachment responses also carry `Content-Security-Policy: default-src 'none'; sandbox`, `Content-Disposition: inline` and `Cross-Origin-Resource-Policy: same-origin`; size/type limits on uploads (default 5 MB, images only); attachments are decoded and re-encoded as WebP, untrusted image bytes are parsed only by memory-safe (Rust) decoders, and uploads are rate limited (§13).
- **Privacy**: reflection authorship is never exposed through the API or WS. Logs never contain reflection content or tokens.
- **Accessibility**: WCAG 2.1 AA. Every drag-and-drop interaction has a keyboard alternative (e.g. a "Move to…" menu).
- **Browser support**: latest two versions of Chrome, Firefox, Safari and Edge.
- **Responsiveness**: usable on desktop and tablet. Mobile supports at least Reflect, Vote and the task board.
- **Observability**: structured logs, Prometheus metrics (HTTP, WS connections, events/sec), health probes.
- **Internationalization**: UI strings externalized (vue-i18n). English first.
