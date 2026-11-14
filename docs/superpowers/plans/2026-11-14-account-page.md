# Your account page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Your account" page at `/account`. It lets users edit their display name, upload
or remove a picture, manage their teams and pending invitations, set preferences and log out.

**Architecture:** New `PATCH /me`, `PUT|DELETE /me/picture` and `GET /users/{id}/picture`
endpoints. Pictures are stored in PostgreSQL (`user_pictures`), and a `users.picture_version`
column lets every DTO built from a user carry `pictureVersion` at no extra cost. On the frontend,
`AccountView` is built from four section components inside the app shell, which now also works
without a `:teamId` route parameter. Avatars load pictures through an authenticated fetch as
`blob:` URLs.

**Tech Stack:** Rust (Axum 0.8, SeaORM 2, utoipa 6, sha2), Vue 3 + Pinia + vue-i18n, orval,
Vitest, Playwright.

**Spec:** `docs/superpowers/specs/2026-11-14-account-and-tasks-board-design.md` (design), and
`docs/specifications.md` once Task 1 has edited it.

## Global Constraints

- Hierarchy of truth: spec > ADRs > code. Task 1 edits the spec before any code.
- Display name: trimmed, 1–80 characters. The error code is `invalid_display_name`.
- Pictures: PNG, JPEG, GIF or WebP, detected from magic bytes; at most 5 MB (5 × 1024 × 1024 bytes).
  Error codes: `unsupported_picture_type` (415) and `picture_too_large` (413).
- A picture is visible to its owner and to anyone sharing a team with them. Anyone else gets 404
  `picture_not_found`.
- Picture responses carry `Cache-Control: private, immutable, max-age=31536000` and `ETag` set to
  the full sha256 hex. Every other API response keeps `no-store`.
- CSP stays `img-src 'self' data: blob:`. Pictures are rendered from `blob:` URLs only.
- UI: tokens from `frontend/src/styles/tokens.css`, never hex. Use real `<button>`, `<a href>`,
  and `<input>` with `<label>`; 44 px targets; inline stroke SVG icons through `AppIcon`; every
  string in `frontend/src/i18n/en.ts`.
- Copy is taken from `.design/project/Account-L.dc.html`, except:
  - Log out text: "Ends your session on this device."
  - Email hint for SSO users: "Comes from your organization's sign-in and can't be changed here.
    Invitations are matched to this address."
  - Email hint for local users: "Invitations are matched to this address."
  - "Signed in with **single sign-on**" or "Signed in with **username and password**". The
    config holds no provider name, only button text.
  - "Use sign-in picture" is replaced by "Remove picture".
- After any endpoint or DTO change, run `npm run api:generate` and commit
  `frontend/openapi.json` and `frontend/src/api/generated.ts`.
- Commits are Conventional Commits, made on branch `feat/account-and-tasks`.

## Review Focus

1. **Name edits survive SSO sign-in.** SSO sign-in after a manual rename keeps the manual
   name. Covered in Task 3 by `sso_sign_in_keeps_a_custom_display_name`.
2. **Upload lies about its type.** A file sent as `image/png` that isn't a PNG is rejected,
   because the type comes from magic bytes and the header is ignored. Covered in Task 4 by
   `rejects_bytes_that_are_not_an_image`.
3. **A file just over 5 MB.** It is rejected with 413 `picture_too_large`, not a generic axum
   413 without problem JSON. Covered in Task 4 by `rejects_pictures_over_5_mb`.
4. **Picture changes, old one cached.** The image URL carries `?v=<pictureVersion>`, so a new
   picture shows without reload in the top bar and on the account page. Covered in Task 6 by
   `UserAvatar reloads when pictureVersion changes`.
5. **Last admin leaves from the account page.** The page shows the server's `last_admin`
   error, and a sole member gets the "deletes the team" confirmation. Covered in Task 9 by
   `TeamsSection shows last_admin` and `TeamsSection warns a sole member`.

---

## File map

Backend:
- `backend/migration/src/m20261114_000001_account.rs` (new) and `lib.rs`: the schema.
- `backend/src/entities/user.rs` (+2 fields) and `entities/user_picture.rs` (new).
- `backend/src/domain/users.rs`: display name rules and the SSO sync tweak.
- `backend/src/domain/pictures.rs` (new): type sniffing, storage and access check.
- `backend/src/api/me.rs`: `PATCH /me`, the picture endpoints and the new `MeResponse` fields.
- `backend/src/api/mod.rs`: `UserDto` and `PersonDto` get `picture_version`.
- `backend/src/app.rs` and `openapi.rs`: routes and the cache-header split.
- `backend/tests/account.rs` (new) and `tests/sso_linking.rs`: tests.

Frontend:
- `src/lib/images.ts` (new): authenticated blob cache.
- `src/components/UserAvatar.vue`: picture support.
- `src/composables/usePendingInvitations.ts` (new): shared by `UserMenu` and the account page.
- `src/composables/useLeaveTeam.ts` (new): shared by `MembersSection` and the account page.
- `src/components/layout/AppShell.vue`: team from the route or the remembered team.
- `src/router/index.ts`: `/account`.
- `src/features/account/` (new): `AccountView.vue`, `ProfileSection.vue`, `TeamsSection.vue`,
  `InvitationsSection.vue`, `PreferencesSection.vue`, `AccountPanel.vue` (the panel chrome).
- `src/i18n/en.ts`: an `account` namespace.
- Tests: `frontend/tests/AccountView.test.ts`, `frontend/tests/avatar.test.ts`,
  `frontend/tests/helpers.ts`.
- E2E: `tests/e2e/account/account.spec.ts`, `tests/e2e/pages/AccountPage.ts`,
  `tests/e2e/api.ts`.

---

### Task 1: Spec edits

**Files:** Modify `docs/specifications.md`.

- [ ] **Step 1:** In §3.2, replace the last bullet with:
  "`email` is synced from the provider's claims on each SSO sign-in. The display name is synced
  too, until the user sets their own on their account page. The `picture` claim is not
  displayed: provider-hosted images are blocked by the Content Security Policy."
- [ ] **Step 2:** In §4, add these bullets:
  - "**Your account** page (`/account`, from the user menu): display name (editable, 1–80
    characters), picture (upload PNG, JPEG, GIF or WebP up to 5 MB, or remove; initials
    otherwise), email (read-only), sign-in method, member since, the user's teams (open, leave,
    create), pending invitations (accept, decline), preferences (theme, language, team opened by
    default on this browser) and log out."
  - "A user's picture is visible to themselves and to members of teams they share."
- [ ] **Step 3:** In §7.1, change the user-menu bullet to "User menu on the right: avatar,
  name, pending invitations, theme, **Your account**, logout."
- [ ] **Step 4:** In §13, give `users` the columns `display_name_custom BOOL DEFAULT false` and
  `picture_version NULL`, and add
  `user_pictures(user_id PK → users, content_type, size_bytes, sha256, data BYTEA, created_at)`.
- [ ] **Step 5:** In §14.1, add these rows:
  - `PATCH /me` — `{ displayName }`
  - `PUT /me/picture` — raw image body
  - `DELETE /me/picture`
  - `GET /users/{id}/picture` — owner or teammates

  In §15.2, add the route `/account → your account`.
- [ ] **Step 6:** Commit: `docs(spec): add the account page`.

### Task 2: Schema and entities

**Files:**
- Create `backend/migration/src/m20261114_000001_account.rs` and `backend/src/entities/user_picture.rs`.
- Modify `backend/migration/src/lib.rs`, `backend/src/entities/{mod.rs,user.rs}`,
  `backend/tests/common/mod.rs` (`create_user`) and `backend/src/seed.rs`, so that ActiveModel
  inserts compile.

**Interfaces:**
- Produces `user::Model.display_name_custom: bool` and `user::Model.picture_version: Option<String>`.
- Produces `user_picture::Model { user_id: Uuid, content_type: String, size_bytes: i32, sha256: String, data: Vec<u8>, created_at }`.

- [ ] **Step 1:** Write the migration in the raw-SQL style of
  `m20261113_000002_open_invitation_unique.rs`:
  - UP:
    ```sql
    ALTER TABLE users ADD COLUMN display_name_custom BOOLEAN NOT NULL DEFAULT false,
                      ADD COLUMN picture_version TEXT NULL;
    CREATE TABLE user_pictures (
      user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
      content_type TEXT NOT NULL, size_bytes INTEGER NOT NULL, sha256 TEXT NOT NULL,
      data BYTEA NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now());
    ```
  - DOWN: the reverse.
  - Register it last in `Migrator::migrations()`.
- [ ] **Step 2:** Add the entity fields, and set them (`false`/`None`) wherever a
  `user::ActiveModel` is built.
- [ ] **Step 3:** Run `cd backend && cargo test --test sso_linking`. Expected: PASS, which shows
  the migrations apply and the inserts compile.
- [ ] **Step 4:** Commit: `feat(account): add display name flag and user pictures table`.

### Task 3: Editable display name

**Files:**
- Modify `backend/src/domain/users.rs`, `backend/src/api/me.rs`, `backend/src/app.rs` and
  `backend/src/openapi.rs`.
- Test: create `backend/tests/account.rs` and modify `backend/tests/sso_linking.rs`.

**Interfaces:**
- Produces `pub const DISPLAY_NAME_MAX: usize = 80`.
- Produces `pub fn validate_display_name(raw: &str) -> Result<String, AppError>`.
- Produces `pub async fn update_display_name(db: &impl ConnectionTrait, user: user::Model, raw: &str) -> Result<user::Model, AppError>`.
- Produces the handler `api::me::update_me`, `PATCH /api/v1/me`, which takes
  `UpdateMeRequest { display_name: String }` (camelCase) and returns `200 UserDto`.

- [ ] **Step 1: Failing tests** in `account.rs`:
  - `renames_the_current_user`: PATCH `{displayName:"  Alice R. "}` → 200, `displayName ==
    "Alice R."`, and `GET /me` shows the new name.
  - `rejects_blank_and_overlong_names`: `"   "` and 81 × `"a"` → 400 with `code ==
    "invalid_display_name"`. 80 × `"a"` → 200.
  - `rename_requires_auth`: no token → 401.
- [ ] **Step 2: Failing test** `sso_sign_in_keeps_a_custom_display_name` in
  `sso_linking.rs`: link alice, PATCH the name to `"Ali"`, sign in through SSO again with name
  `"Alice From SSO"`, and assert the name is `"Ali"`.
- [ ] **Step 3:** Run `cargo test --test account --test sso_linking`. Expected: FAIL (404
  route, and the name gets overwritten).
- [ ] **Step 4: Implement.** `update_display_name` sets `display_name`, sets
  `display_name_custom = true` and bumps `updated_at`. In `sign_in_with_identity`, set
  `display_name` from the claim only when `!display_name_custom`. Add the handler
  (`utoipa::path`, responses 200/400/401), route it with `.route("/me", get(...).patch(...))`
  and add it to `openapi.rs` `paths`.
- [ ] **Step 5:** Run the same tests. Expected: PASS.
- [ ] **Step 6:** Commit: `feat(account): let users edit their display name`.

### Task 4: Picture upload, removal and download

**Files:**
- Create `backend/src/domain/pictures.rs` (unit tests inside).
- Modify `backend/src/domain/mod.rs`, `backend/src/api/{me.rs,mod.rs}`, `backend/src/app.rs`,
  `backend/src/openapi.rs` and `backend/src/error.rs` (new variants, 413/415).
- Test: `backend/tests/account.rs`.

**Interfaces:**
- Produces `pub const PICTURE_MAX_BYTES: usize = 5 * 1024 * 1024`.
- Produces `pub fn sniff_image_type(bytes: &[u8]) -> Option<&'static str>`, which returns
  `image/png | image/jpeg | image/gif | image/webp`.
- Produces `pub async fn set_picture(db, user: user::Model, bytes: Vec<u8>) -> Result<user::Model, AppError>`.
- Produces `pub async fn remove_picture(db, user: user::Model) -> Result<user::Model, AppError>`.
- Produces `pub async fn picture_for(db, viewer: Uuid, owner: Uuid) -> Result<user_picture::Model, AppError>`.
- Produces `UserDto.picture_version` and `PersonDto.picture_version: Option<String>`, both
  serialized as `pictureVersion`. `PersonDto` built by hand in `PendingInvitationDto` uses `None`.
- Produces the handlers:
  - `api::me::put_picture`: `PUT /api/v1/me/picture`, raw body → 200 `UserDto`.
  - `api::me::delete_picture`: `DELETE /api/v1/me/picture` → 200 `UserDto`.
  - `api::me::get_picture`: `GET /api/v1/users/{user_id}/picture` → 200 image bytes.

- [ ] **Step 1: Failing unit tests** in `pictures.rs`:
  - `sniff_image_type` recognises these magic bytes:
    - PNG `89 50 4E 47 0D 0A 1A 0A`
    - JPEG `FF D8 FF`
    - GIF `GIF87a` / `GIF89a`
    - WebP `RIFF????WEBP`
  - It returns `None` for `b"<svg"` and for an empty slice.
- [ ] **Step 2: Failing integration tests** in `account.rs`. Add a helper
  `put_bytes(app, path, token, content_type, bytes)` that builds the request like `call`, and a
  1×1 PNG constant.
  - `uploads_and_serves_a_picture`: PUT the PNG → 200, `pictureVersion` = the first 16 hex
    characters of its sha256. Alice's own GET returns 200, `content-type: image/png`, the same
    bytes, `etag: "<sha256>"`, and a `cache-control` containing `private` and `immutable`.
  - `teammates_see_the_picture_strangers_do_not`: bob in the same team (create the team, join
    through the link as in `teams.rs`) → 200. A new user outside the team → 404 with `code ==
    "picture_not_found"`.
  - `rejects_bytes_that_are_not_an_image`: `b"not an image"` sent with `image/png` → 415
    `unsupported_picture_type`.
  - `rejects_pictures_over_5_mb`: PNG magic bytes followed by zeros up to 5 MiB + 1 byte →
    413 `picture_too_large`, as problem JSON.
  - `removes_the_picture`: DELETE → 200 with `pictureVersion == null`, and GET → 404.
  - `me_lists_picture_version`: `GET /me` → `user.pictureVersion` matches.
- [ ] **Step 3:** Run `cargo test --lib pictures` and `cargo test --test account`. Expected:
  FAIL.
- [ ] **Step 4: Implement the domain.** `set_picture`:
  1. Checks the size first (`AppError::PayloadTooLarge`, a new variant → 413
     `picture_too_large`).
  2. Sniffs the type (a new `AppError::UnsupportedMediaType` → 415
     `unsupported_picture_type`).
  3. Upserts `user_pictures` (`ON CONFLICT (user_id) DO UPDATE`) and sets
     `users.picture_version = sha256[..16]`, in one transaction.

  `picture_for` is allowed when `viewer == owner` or when this query matches:

  ```sql
  EXISTS(SELECT 1 FROM team_members a JOIN team_members b ON a.team_id=b.team_id
         WHERE a.user_id=$1 AND b.user_id=$2)
  ```

  Otherwise, or when there is no row, it returns `NotFound("picture_not_found")`.
- [ ] **Step 5: Implement the routes in `app.rs`:**
  - Give `PUT /me/picture` its own `DefaultBodyLimit::max(PICTURE_MAX_BYTES + 1)`, so the
    handler, not axum, rejects oversize bodies with problem JSON. The handler takes
    `body: axum::body::Bytes`.
  - Build `GET /users/{user_id}/picture` as a separate `Router` that is `.merge`d into `/api/v1`
    **after** the `no-store` `SetResponseHeaderLayer`, so its own `Cache-Control` survives.
  - Add the three handlers to `openapi.rs`. The request body is
    `content = Vec<u8>, content_type = "image/*"`.
- [ ] **Step 6:** Run the tests. Expected: PASS.
- [ ] **Step 7:** Commit: `feat(account): upload, remove and serve user pictures`.

### Task 5: Account details on `/me`, then regenerate the client

**Files:**
- Modify `backend/src/api/me.rs` and `backend/tests/account.rs`.
- Regenerate `frontend/openapi.json` and `frontend/src/api/generated.ts`.
- Modify `frontend/src/api/client.ts` to export the new types.

**Interfaces:**
- Produces `MeResponse` with `member_since: OffsetDateTime` (rfc3339) and `sign_in_method:
  SignInMethod`, where `#[serde(rename_all="lowercase")] enum SignInMethod { Password, Sso }`.
  It is `sso` when the user has a `user_identities` row, else `password`.
- Produces the client exports `SignInMethod`, `UpdateMeRequest`, `api.updateMe`,
  `api.putPicture`, `api.deletePicture` and `api.getGetPictureUrl`.

- [ ] **Step 1: Failing test** `me_reports_sign_in_method_and_member_since`: alice → `password`,
  and `memberSince` parses as RFC 3339. After SSO linking (reuse `identity()` from
  `sso_linking.rs` by copying it into `account.rs`) → `sso`.
- [ ] **Step 2:** Run it. Expected: FAIL.
- [ ] **Step 3:** Implement it, then run `npm run api:generate`. Re-export the new names from
  `client.ts`.
- [ ] **Step 4:** Run `cargo test --test account` and `npm run lint`. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(account): expose sign-in method and member since`.

### Task 6: Pictures in avatars

**Files:**
- Create `frontend/src/lib/images.ts`.
- Modify `frontend/src/components/UserAvatar.vue` and every `<UserAvatar>` caller that has a
  `UserDto` or `PersonDto` (pass `:picture-version`).
- Modify `frontend/tests/helpers.ts`: `mockFetch` must not `JSON.parse` non-JSON bodies. Only
  parse when the request `Content-Type` includes `json`; otherwise record the body as
  `ArrayBuffer`. Let a route return a `Blob` body.
- Test: create `frontend/tests/avatar.test.ts`.

**Interfaces:**
- Produces `loadImage(url: string): Promise<string>`, which resolves to a `blob:` URL. It is
  cached per URL for the session, fetched with `authFetch` from `@/api/fetcher`, and rejects on
  a non-200 response.
- Produces `clearImageCache(): void`, which revokes every URL. The session store's `clear()`
  calls it.
- Produces the `UserAvatar` prop `pictureVersion?: string | null`. When set, it shows
  `<img :src="blobUrl" alt="">` from `/api/v1/users/{userId}/picture?v={pictureVersion}`, with
  initials until it loads and after an error.

- [ ] **Step 1: Failing tests:**
  - `UserAvatar shows initials without a picture`.
  - `UserAvatar shows the picture once loaded`: mock `GET /api/v1/users/<id>/picture` → a PNG
    blob; stub `URL.createObjectURL` → `"blob:x"`; expect `img[src="blob:x"]`.
  - `UserAvatar falls back to initials when the picture fails` (404).
  - `UserAvatar reloads when pictureVersion changes`: the second version fetches `?v=b`.
- [ ] **Step 2:** Run `cd frontend && npx vitest run tests/avatar.test.ts`. Expected: FAIL.
- [ ] **Step 3:** Implement it. Pass `pictureVersion` from `AppTopBar`/`UserMenu`
  (`session.user`) and from `MembersSection` (`member.user`).
- [ ] **Step 4:** Run `npm run test --workspace @retro/frontend`. Expected: PASS, including the
  existing suites.
- [ ] **Step 5:** Commit: `feat(account): show user pictures in avatars`.

### Task 7: `/account` route, shell without a team param, user menu link

**Files:**
- Modify `frontend/src/router/index.ts`, `frontend/src/components/layout/AppShell.vue`,
  `frontend/src/components/layout/UserMenu.vue` and `frontend/src/i18n/en.ts`.
- Test: `frontend/tests/router.test.ts` and `frontend/tests/AppShell.test.ts`.

**Interfaces:**
- Produces the route `{ path: '/account', component: AppShell, meta: { requiresAuth: true },
  children: [{ path: '', name: 'account', component: AccountView }] }`. `AccountView` is a
  stub `<h1>` until Task 8.
- `AppShell`: `teamId = route.params.teamId ?? teams.defaultTeam()?.id`. The nav is rendered
  only when a team id exists. The `RouterView` key stays `teamId ?? route.name`.
- `UserMenu`: a `RouterLink` "Your account" (`userMenu.account`) to `{ name: 'account' }`,
  placed above Log out, which closes the menu.

- [ ] **Step 1: Failing tests:**
  - `router.test.ts`: `/account` signed in → renders the `Your account` heading; signed out →
    `/login?redirect=/account`.
  - `AppShell.test.ts`: on `/account` with the remembered team, the nav links point to that
    team. With no teams there is no `nav[aria-label="Team"]`. The user menu has a "Your account"
    link to `/account`.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement it.
- [ ] **Step 4:** Run `npm run test --workspace @retro/frontend`. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(account): add the account route and menu link`.

### Task 8: Account page — Profile section

**Files:**
- Create `frontend/src/features/account/{AccountView.vue,AccountPanel.vue,ProfileSection.vue}`.
- Modify `frontend/src/stores/session.ts` and `frontend/src/i18n/en.ts` (`account.*`).
- Test: create `frontend/tests/AccountView.test.ts`.

**Interfaces:**
- Produces the session store method `setUser(user: User): void`.
- Produces `AccountView`, which loads `GET /me` itself for `memberSince` and `signInMethod`
  through `api.me()`. The teams store keeps its own copy, and the page reads teams and
  invitations from the store.
- Produces `AccountPanel` with props `{ id: string; title: string; description?: string }`, a
  default slot and an `actions` slot. It is the section chrome: radius 14, `--surface`, panel
  shadow, `aria-labelledby`.
- Layout follows `Account-L`: the header ("Your account" / "Your profile, teams and
  preferences"), then the Profile, Your teams, Pending invitations (only when there are any),
  Preferences and Log out panels.
- `ProfileSection` contents:
  - The avatar at 96 px (check the size in the artboard) with a "Change picture" icon button.
  - "Upload picture" (a hidden `<input type="file" accept="image/png,image/jpeg,image/gif,image/webp">`
    triggered by both buttons).
  - "Remove picture" when `pictureVersion` is set.
  - The hint "PNG, JPEG, GIF or WebP · up to 5 MB".
  - A `TextField` "Display name" with the hint "Shown to your teams in meetings, on tasks and in
    mentions."
  - A read-only email row with the copy chosen by `signInMethod`.
  - "Signed in with …" and "Member since {date}" (`d MMM yyyy` through `Intl.DateTimeFormat('en-GB')`).
  - Cancel (resets the field) and Save changes (disabled while unchanged or blank).
- Behaviour:
  - Uploading a file over 5 MB shows the error locally, without a request.
  - Server errors show `errors.<code>` in an `AppAlert`.
  - Success calls `session.setUser(res.data)`.
- Add the i18n codes: `errors.invalid_display_name`, `errors.picture_too_large` and
  `errors.unsupported_picture_type`.

- [ ] **Step 1: Failing tests:**
  - `saves a new display name`: PATCH body `{displayName:"Ali"}`, and the top bar shows "Ali".
  - `Save is disabled until the name changes`.
  - `shows invalid_display_name from the server`.
  - `uploads a picture`: PUT `/api/v1/me/picture` with the file's type as `Content-Type`.
  - `rejects a file over 5 MB without uploading`.
  - `Remove picture appears only with a picture`.
  - `shows the SSO email hint for sso users and the local hint otherwise`.
- [ ] **Step 2:** Run `npx vitest run tests/AccountView.test.ts`. Expected: FAIL.
- [ ] **Step 3:** Implement it. `api.putPicture` is generated from a binary body; if orval's
  signature doesn't accept a `Blob` with its own type, call `apiFetch` with
  `{ method: 'PUT', body: file, headers: { 'Content-Type': file.type } }` in a small wrapper in
  `client.ts` called `uploadPicture(file: File)`.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(account): profile section`.

### Task 9: Teams and invitations sections

**Files:**
- Create `frontend/src/composables/{usePendingInvitations.ts,useLeaveTeam.ts}` and
  `frontend/src/features/account/{TeamsSection.vue,InvitationsSection.vue}`.
- Modify `frontend/src/components/layout/UserMenu.vue` and
  `frontend/src/features/teams/settings/MembersSection.vue` so both use the composables, plus
  `en.ts`.
- Test: `frontend/tests/AccountView.test.ts`. The existing `AppShell.test.ts` and
  `TeamSettingsView.test.ts` must stay green.

**Interfaces:**
- Produces `usePendingInvitations()`, returning `{ invitations: ComputedRef<PendingInvitationDto[]>, busy: Ref<string|null>, error: Ref<string|null>, accept(token): Promise<void>, decline(token): Promise<void> }`.
  `accept` navigates to the joined team's meetings, the same as `UserMenu` does today.
- Produces `useLeaveTeam()`, returning `{ pending: Ref<TeamRefDto & { soleMember: boolean } | null>, dialog: ComputedRef<{title,text,confirm}|null>, error: Ref<string|null>, ask(team, soleMember), confirm(userId): Promise<boolean>, cancel() }`.
  It reuses the `settings.leave.*` i18n keys and `api.removeMember`, and reloads the teams store
  on success.
- `TeamsSection`:
  - Title "Your teams · {n}", hint "Last admins need to promote someone else before leaving.",
    and a "Create team" button that opens `CreateTeamDialog`.
  - Each row has a `TeamTile`, the name, a "Current" tag for the remembered team,
    "{n} members", an Admin/Member tag, `Open` (a `RouterLink` to team-meetings) and `Leave`.
  - `soleMember` is `memberCount === 1`. After leaving, the page stays on `/account`.
- `InvitationsSection`:
  - Title "Pending invitations · {n}" and "Sent to {email}".
  - Rows show a `TeamTile`, the team name, "Invited by {name} · expires {relative}" (`relativeFromNow`
    from `lib/time.ts`, e.g. "in 6 days"), plus Decline and Accept.

- [ ] **Step 1: Failing tests:**
  - `lists teams with role and member count`.
  - `Open goes to the team's meetings`.
  - `TeamsSection shows last_admin`: DELETE → 409 `last_admin`, and the dialog shows
    `errors.last_admin`.
  - `TeamsSection warns a sole member`: the dialog uses `settings.leave.lastTitle`.
  - `accepting an invitation opens the team`.
  - `declining removes the row`.
  - `the invitations panel is hidden when there are none`.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement the sections, then switch `UserMenu` and `MembersSection` to the
  composables.
- [ ] **Step 4:** Run `npm run test --workspace @retro/frontend`. Expected: PASS (all suites).
- [ ] **Step 5:** Commit: `feat(account): teams and invitations sections`.

### Task 10: Preferences and log out

**Files:**
- Create `frontend/src/features/account/PreferencesSection.vue`.
- Modify `AccountView.vue` and `en.ts`.
- Test: `frontend/tests/AccountView.test.ts`.

**Interfaces:**
- Theme: a segmented group (`role="group"`, `aria-pressed`) with Light, Dark and System, using
  `readThemePreference` and `applyThemePreference` from `lib/theme.ts`. The hint is "System
  follows your device setting."
- Language: a `<select>` labelled "Language" with the single option "English", and the hint
  "More languages are coming."
- Default team:
  - A `<select>` labelled "Default team" listing the teams store. Its value is
    `teams.defaultTeam()?.id`, and `@change` calls `teams.remember(id)`.
  - Hint: "Retro opens the team you used last on this browser."
  - The control is hidden when there are no teams.
- Log out panel: the "Log out" heading, the text "Ends your session on this device.", and a
  button that calls `session.logout()` and then `router.replace({ name: 'login' })`.

- [ ] **Step 1: Failing tests:**
  - `switches theme`: clicking Dark sets `<html data-theme="dark">`.
  - `changing the default team remembers it`: the `localStorage` key
    `retro.lastTeam.<userId>`.
  - `logs out`: POST `/api/v1/auth/logout`, then `/login`.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement it.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(account): preferences and log out`.

### Task 11: Visual pass against the artboards

**Files:** the `frontend/src/features/account/*.vue` styles.

- [ ] **Step 1:** Compare every section with `.design/project/Account-L.dc.html` and
  `Account-D.dc.html`: spacing, sizes and weights. Replace any hex with a token. Check that
  nothing scrolls horizontally at 390 px width.
- [ ] **Step 2:** Run `npm run lint`. Expected: PASS.
- [ ] **Step 3:** Commit: `style(account): match the artboards`.

### Task 12: End-to-end test

**Files:**
- Create `tests/e2e/account/account.spec.ts` and `tests/e2e/pages/AccountPage.ts`.
- Modify `tests/e2e/api.ts`: `resetAccount` also PATCHes `/api/v1/me` back to the
  `fixtures.ts` name and DELETEs `/api/v1/me/picture`.
- Add a PNG fixture: `tests/e2e/assets/avatar.png`, a 64×64 PNG generated once and committed.

**Interfaces:**
- `AccountPage`:
  - Fields: `displayName: Locator`, `save: Locator`, `uploadInput: Locator`.
  - Methods: `goto()` (through the user menu's "Your account" link), `rename(name)`,
    `uploadPicture(path)`, `chooseTheme('Light'|'Dark'|'System')`, `leave(team)`.

- [ ] **Step 1:** Write `account.spec.ts`, with `beforeEach` running `resetAccount('alice')`
  and creating a team "Account E2E" through `AppShell.addTeam`:
  - Rename to "Alice E2E", reload, and the field and top bar show "Alice E2E".
  - Upload `avatar.png`, and the top-bar avatar contains an `img` with a `blob:` src.
  - Click Dark, and `html` has `data-theme="dark"`.
  - Leave "Account E2E" as its sole member: the confirmation is shown, then the team row is
    gone.
- [ ] **Step 2:** Run `scripts/build.sh && scripts/restart.sh`, then
  `scripts/test-playwright.sh https://$(scripts/dev-host.sh) tests/e2e/account`. Expected:
  PASS. Run it again for the whole suite. Expected: PASS.
- [ ] **Step 3:** Commit: `test(account): end-to-end happy path`.

### Task 13: Verification

- [ ] Run `npm run lint`. Expected: exit 0.
- [ ] Run `npm run test`. Expected: all backend and frontend tests pass.
- [ ] Run `git status`. Expected: a clean tree, with nothing left from `api:generate`.
