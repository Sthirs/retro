# Your account page and Tasks board — design

Date: 2026-11-14 · Status: approved

Design reference: canvas artboards `Account-L/-D` and `Tasks-L/-D` (local copy in
`.design/project/`, tokens in `.design/brief.md`).

## Intent

- **Your account** gives users one place for their profile (name, picture), their teams, pending
  invitations, preferences and log out. It is reached from the user menu.
- **Tasks board** replaces the "coming soon" placeholder at `/teams/:teamId/tasks` with the team's
  four-column board (spec §10), ready for the Discuss phase to feed it.

## Goals

- Both screens match the artboards in light and dark, using tokens only.
- Account: everything in the design, except "Use sign-in picture".
- Tasks: list, filter, drag and drop, keyboard "Move to…", edit (rich text with @mentions) and
  delete, with server-side permissions from spec §12.

## Non-goals

- Creating tasks. Spec §10.1 allows that only in Discuss, which does not exist yet.
- Live board updates. These arrive with the real-time hub (spec §11). Until then the board
  refetches on window focus.
- Pasted images in task content (attachments), and meeting links on cards (meetings).
- Signing out of the identity provider. Log out ends the Retro session only.
- Languages other than English.

## Anti-goals

- Loosening the CSP to show provider-hosted pictures.
- Any task-creation path outside meetings in the product. The e2e seeding endpoint is dev-only.

## Constraints

- Spec > ADRs > code. The spec is edited first (§3.2, §4, §7.1, §10.1 note, §13, §14.1, §15.2).
- CSP `img-src 'self' data: blob:`. Pictures are served by the backend behind auth, and the
  frontend loads them as `blob:` URLs (spec §15.4).
- Images are stored only in PostgreSQL, limited to 5 MB and PNG, JPEG, GIF or WebP (spec §13).
- OpenAPI is the contract (`npm run api:generate`).
- Every feature gets backend integration tests, Vitest tests and a Playwright happy path.

## Approaches considered

**Tasks scope**

1. *Board and backend without creation (picked).* Build the real tables and endpoints now. The
   board is fed by Discuss later, and tests seed tasks directly.
2. A UI on fixtures only. This is throwaway and can't meet the e2e rule.
3. Defer Tasks to wave 5, after meetings, which is slower to see.
4. Allow temporary creation outside meetings. This contradicts spec §10.1.

**Task editing.** The Tiptap editor is built now (picked) because Reflect and Discuss need it
anyway. The alternative was to defer Edit.

**Provider pictures**

- Copy them into Retro: keeps the CSP, but the server makes outbound fetches.
- Allow `https:` in the CSP: weakens it and lets the provider track views.
- Drop "Use sign-in picture" (picked). Only uploaded pictures are shown.

**Log out.** Retro session only (picked). IdP logout stays a later addition (spec §3.3).

## Decisions

### Account

- Route `/account`, inside the app shell. The shell takes its team from the route, or else from
  the remembered team. A user with no team gets the shell without team nav. *(builder's call,
  confirmed)*
- `PATCH /me {displayName}`: trimmed, 1–80 characters. Saving sets `users.display_name_custom`,
  after which SSO sign-in no longer overwrites the name.
- Pictures go in a new `user_pictures` table (one row per user, BYTEA). The endpoints are
  `PUT /me/picture` (raw body, type checked by magic bytes, at most 5 MB) and
  `DELETE /me/picture`. `GET /users/{id}/picture` is allowed for the user and for anyone sharing
  a team with them; its `ETag` is the sha256 and it is sent with
  `Cache-Control: private, immutable`. `UserDto` and `PersonDto` expose `pictureVersion`.
- "Remove picture" replaces the dropped "Use sign-in picture" and is shown only when a picture
  is set. *(builder's call)*
- `MeResponse` adds `memberSince` and `signInMethod` (`sso` when the user has a linked identity,
  else `password`), plus the SSO provider name for "Signed in with …".
- Teams section: Open; Leave (the existing flow, including the last-admin and
  last-member-deletes-team rules); Create team (the existing dialog).
- Invitations section: the same accept and decline actions as the user menu, through one shared
  composable.
- Preferences:
  - theme, through the existing `lib/theme.ts`;
  - language, a select with English only;
  - default team, a select that sets the browser-remembered team (spec §7.1).
- Log out copy: "Ends your session on this device."

### Tasks

- Tables `tasks` and `task_mentions` follow spec §13. `meeting_id` and `group_id` are nullable
  with no foreign key for now; the meetings migration adds the keys.
- Rich text is stored as server-sanitized HTML (ammonia allowlist), and mentions are mention
  spans carrying `data-id`. The server derives `task_mentions` and keeps only team members. The
  client sanitizes again with DOMPurify. This is recorded as a proposed ADR (rich-text format).
- Ordering uses fractional-index string keys (`COLLATE "C"`), so a move updates one row
  (spec §10.2). *(builder's call)*
- Endpoints follow spec §14.5, except `POST /meetings/{id}/tasks`:
  - `GET /teams/{teamId}/tasks?mentioned=&author=&meeting=` returns `TaskDto` with `canEdit`.
  - `PUT /tasks/{tid}/position {status, beforeId?, afterId?}`.
  - `PATCH /tasks/{tid} {content}`.
  - `DELETE /tasks/{tid}`.
- E2E seeding uses `POST /api/v1/dev/teams/{teamId}/tasks`, mounted only when
  `SEED_DEV_USERS=true` (refused in production) and left out of OpenAPI. It is removed once
  Discuss exists. *(builder's call)*
- UI:
  - Columns: Future, Stuck, Active, Done, each with a count.
  - Drag and drop with vue-draggable-plus. Moves are optimistic and roll back on error.
  - The card menu has Edit and Delete when `canEdit`, and always has Move to…, which appends the
    card to the end of the chosen column.
  - Filters: My tasks (= mentioned me), Mentioned, Author, Meeting (hidden until some task has a
    meeting), and Clear. Filters live in the URL query.
  - The empty state explains that tasks come from the Discuss phase.

## Left to the builder

The component split, store shape, i18n keys, exact error copy, and the responsive behaviour
below the 1440 px artboards (wave 6), as long as phone width doesn't scroll horizontally.
