# Tasks board Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Tasks placeholder at `/teams/:teamId/tasks` with the team's four-column
board. It supports listing, filtering, drag and drop, keyboard "Move to…", rich-text edit with
@mentions, and delete. Creating tasks waits for the Discuss phase.

**Architecture:** The new tables `tasks` and `task_mentions` hold content as server-sanitized
HTML (ammonia), and mentions are derived from mention spans. Order within a column uses
fractional-index string keys, so a move writes one row. The frontend has a `tasks` Pinia store
with optimistic moves, `vue-draggable-plus` columns, and a shared Tiptap `RichTextEditor` that
Discuss will reuse later.

**Tech Stack:** Rust (Axum 0.8, SeaORM 2, utoipa 6, ammonia), Vue 3, Pinia, Tiptap,
DOMPurify, vue-draggable-plus, Vitest, Playwright.

**Spec:** `docs/superpowers/specs/2026-11-14-account-and-tasks-board-design.md` (design), and
`docs/specifications.md` §10, §12, §13 and §14.5.

## Global Constraints

- Columns, in this order: Future, Stuck, Active, Done. The status values are
  `future | stuck | active | done`.
- Permissions (spec §12): any member lists and moves tasks. Only the task's author or a team
  admin edits or deletes it. Non-members get 404 `team_not_found` or `task_not_found`, so the
  API doesn't reveal that the team or task exists.
- Allowed rich-text HTML: `p`, `br`, `strong`, `em`, `a[href]` (schemes `http`, `https`,
  `mailto`; ammonia adds `rel="noopener noreferrer"`), and
  `span[data-type="mention"][data-id][data-label]`. Everything else is stripped.
- Content is at most 10 000 characters after sanitizing, and its text must not be blank.
  Otherwise the error is 400 `invalid_task_content`.
- Mentions stored are the `data-id`s that are UUIDs of **current team members**. Others are
  dropped from `task_mentions`, but their span stays in the HTML.
- Positions use the alphabet `0-9A-Z a-z` (62 symbols, ASCII order) and the column is
  `COLLATE "C"`. Lists are ordered by `(status, position, id)`.
- No task creation endpoint in the product API. The dev seeding route exists only when
  `SEED_DEV_USERS=true` and is left out of OpenAPI.
- UI: tokens only; real buttons, links and labelled inputs; 44 px targets; `AppIcon` SVGs;
  vue-i18n strings. Copy comes from `.design/project/Tasks-L.dc.html`.
- After any endpoint or DTO change, run `npm run api:generate` and commit the generated files.
- Commits are Conventional Commits. Tasks 5–14 add the trailer `Implements ADR-0010` only once
  a human has accepted that ADR; otherwise they leave it out.

## Review Focus

1. **Two people drop a card into the same slot.** Both moves compute the same key. A UNIQUE
   index `(team_id, status, position)` makes the second fail with 409 `stale_position`, and the
   client reloads the board. Covered in Task 6 by `concurrent_identical_keys_conflict` and in
   Task 10 by `move rolls back and reloads on 409`.
2. **Script in task content.** `<img onerror>`, `<script>` and `javascript:` links are stripped
   by the server and again by DOMPurify. Covered in Task 4 by `strips_scripts_and_js_links` and
   in Task 9 by `RichText drops script and handlers`.
3. **Mentioning someone outside the team.** The span stays, but there is no `task_mentions`
   row, so the "Mentioned" filter can't find them. Covered in Task 7 by
   `edit_keeps_only_member_mentions`.
4. **Filtered board, then a drag.** Neighbours sent are the visible ones. The server accepts
   any two tasks from the same column as long as `after < before`. Covered in Task 6 by
   `moves_between_non_adjacent_neighbours`.
5. **Moving a task from another team.** A neighbour id that belongs to another team or column
   gives 400 `invalid_position`. Covered in Task 6 by `rejects_foreign_neighbours`.

---

## File map

Backend:
- `backend/migration/src/m20261114_000002_tasks.rs` (new) and `lib.rs`.
- `backend/src/entities/{task.rs,task_mention.rs}` (new) and `mod.rs`.
- `backend/src/domain/ordering.rs` (new): fractional keys.
- `backend/src/domain/rich_text.rs` (new): sanitize and extract mentions.
- `backend/src/domain/tasks.rs` (new): queries, permissions, move, edit, delete.
- `backend/src/api/tasks.rs` (new): handlers and DTOs.
- `backend/src/api/dev.rs` (new): the seeding route.
- `backend/src/app.rs` and `openapi.rs`.
- `backend/tests/tasks.rs` (new).

Frontend:
- `src/components/{RichText.vue,RichTextEditor.vue}` (new).
- `src/stores/tasks.ts` (new).
- `src/features/tasks/`: `TasksBoardView.vue`, `TaskColumn.vue`, `TaskCard.vue`,
  `TaskMenu.vue`, `TaskFilters.vue`, `EditTaskDialog.vue`.
- `src/router/index.ts` and `src/i18n/en.ts` (a `tasks` namespace).
- Tests: `frontend/tests/{TasksBoardView.test.ts,tasksStore.test.ts,RichText.test.ts}`.

E2E: `tests/e2e/tasks/tasks.spec.ts`, `tests/e2e/pages/TasksPage.ts` and
`tests/e2e/api.ts` (`seedTask`).

---

### Task 1: ADR and spec notes

**Files:** Create `docs/adr/0010-rich-text-format.md` from `docs/adr/template.md`. Modify
`docs/adr/README.md` (the index) and `docs/specifications.md`.

- [ ] **Step 1:** Write the ADR with status `proposed`.
  - Decision: rich text (tasks now, reflections later) is stored as HTML sanitized on the server
    with ammonia, using the allowlist in Global Constraints. Mentions are `span[data-type=mention]`
    elements carrying `data-id`, and the server derives `task_mentions` from them. The client
    sanitizes again with DOMPurify before `v-html`.
  - Alternatives: ProseMirror JSON (needs a renderer on every client and makes server search
    harder) and Markdown (mentions and images need custom syntax).
  - Sources: spec §9.2, §9.5, §10.1, §16.1 (ammonia) and §15.1 (Tiptap, DOMPurify).
- [ ] **Step 2:** Spec edits:
  - §10.1: append "Until the real-time hub (§11) exists, the board refreshes when the window
    regains focus."
  - §10.2: replace "fractional indexing (or sparse integers)" with "fractional-index string keys
    (base 62, compared byte-wise); concurrent moves to the same slot conflict and the client
    reloads".
  - §13: `tasks.position TEXT`, plus `UNIQUE(team_id, status, position)`.
  - §14.5: describe `beforeId` and `afterId`: "the moved task ends up directly after `afterId`
    and before `beforeId`; with neither, at the end of the column".
- [ ] **Step 3:** Commit: `docs: propose ADR-0010 rich-text format and refine task ordering`.

### Task 2: Schema and entities

**Files:**
- Create `backend/migration/src/m20261114_000002_tasks.rs` and
  `backend/src/entities/{task.rs,task_mention.rs}`.
- Modify `backend/migration/src/lib.rs` and `backend/src/entities/mod.rs`.

**Interfaces:**
- Produces `task::Model { id, team_id, meeting_id: Option<Uuid>, group_id: Option<Uuid>, author_id, content: String, status: TaskStatus, position: String, created_at, updated_at }`.
- Produces `TaskStatus`: a `DeriveActiveEnum` over the Postgres enum `task_status`, with the
  variants `Future, Stuck, Active, Done` and `#[serde(rename_all = "lowercase")]`, plus
  `ToSchema`. Follow `team_member::Role` for the pattern.
- Produces `task_mention::Model { task_id, user_id }`.

- [ ] **Step 1:** Write the migration (raw SQL):

  ```sql
  CREATE TYPE task_status AS ENUM ('future','stuck','active','done');
  CREATE TABLE tasks (
    id UUID PRIMARY KEY,
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    meeting_id UUID NULL, group_id UUID NULL,   -- FKs added by the meetings migration
    author_id UUID NOT NULL REFERENCES users(id),
    content TEXT NOT NULL,
    status task_status NOT NULL,
    position TEXT COLLATE "C" NOT NULL,
    created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (team_id, status, position));
  CREATE TABLE task_mentions (
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, user_id));
  CREATE INDEX task_mentions_user_idx ON task_mentions (user_id);
  ```

  DOWN drops the tables and then the type.
- [ ] **Step 2:** Run `cd backend && cargo test --test teams`. Expected: PASS, which shows the
  migrations apply and team deletion still cascades.
- [ ] **Step 3:** Commit: `feat(tasks): add tasks and task_mentions tables`.

### Task 3: Fractional position keys

**Files:** Create `backend/src/domain/ordering.rs` (unit tests inside) and modify
`backend/src/domain/mod.rs`.

**Interfaces:**
- Produces `pub fn key_between(after: Option<&str>, before: Option<&str>) -> Option<String>`.
  It returns `None` when `after >= before`, or when either key is empty or ends in `'0'`
  (both are invalid keys).

- [ ] **Step 1: Failing tests:**
  - `key_between(None, None) == "V"`.
  - `key_between(Some("V"), None) > "V"`.
  - `key_between(None, Some("V")) < "V"`.
  - `key_between(Some("a"), Some("b"))` lies strictly between them (it gives `"aV"`).
  - `key_between(Some("a"), Some("a")) == None`.
  - `key_between(Some("b"), Some("a")) == None`.
  - Property: 1 000 inserts that each go just after the previous key, and 1 000 that each go
    just before the first, all stay strictly ordered, never end in `'0'`, and stay under 200
    characters.
- [ ] **Step 2:** Run `cargo test --lib ordering`. Expected: FAIL.
- [ ] **Step 3:** Implement the midpoint over digits 0..62. `a` defaults to the empty string,
  read as digits that are all 0, and `b` defaults to "1.0", read as digit 62.

  ```text
  mid(a, b):
    if b is Some: n = length of the common prefix of a (padded with '0') and b
      if n > 0: return b[..n] + mid(a[n..], Some(b[n..]))
    da = digit(a[0]) or 0;  db = digit(b[0]) or 62
    if db - da > 1: return [digit((da + db + 1) / 2)]
    if b is Some and b.len() > 1: return b[..1]
    return [a[0] or '0'] + mid(a[1..], None)
  ```
- [ ] **Step 4:** Run it again. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(tasks): fractional position keys`.

### Task 4: Rich-text sanitizer

**Files:** Modify `backend/Cargo.toml` (`ammonia`, latest 4.x, exact as with other deps) and
`backend/src/domain/mod.rs`. Create `backend/src/domain/rich_text.rs` (unit tests inside).

**Interfaces:**
- Produces `pub struct Sanitized { pub html: String, pub mention_ids: Vec<Uuid> }`, with
  mentions de-duplicated in order of appearance.
- Produces `pub const CONTENT_MAX_CHARS: usize = 10_000`.
- Produces `pub fn sanitize(raw: &str) -> Result<Sanitized, AppError>`, which returns
  `Invalid("invalid_task_content")` when the text is blank or the HTML is too long.

- [ ] **Step 1: Failing tests:**
  - `keeps_allowed_markup`: `<p><strong>a</strong> <em>b</em> <a href="https://x.dev">l</a></p>`
    keeps every tag, and the `a` gains `rel="noopener noreferrer"`.
  - `strips_scripts_and_js_links`: `<script>`, `<img src=x onerror=…>`, `<a href="javascript:…">`
    and `style` attributes are all removed.
  - `extracts_mentions`: two spans with the same uuid and one with `data-id="nope"` give one id.
  - `rejects_blank_and_overlong`: `<p> </p>` and 10 001 characters → `invalid_task_content`.
- [ ] **Step 2:** Run `cargo test --lib rich_text`. Expected: FAIL.
- [ ] **Step 3:** Implement it with `ammonia::Builder`:
  - `tags`, `url_schemes`, `link_rel`, and per-tag attributes (`span: data-type, data-id,
    data-label`).
  - An `attribute_filter` that drops `span` attributes other than `data-type="mention"`, and
    collects `data-id` values that parse as `Uuid` into a `RefCell<Vec<Uuid>>`.
  - The blank test uses a second builder with no tags, then `.trim().is_empty()`.
- [ ] **Step 4:** Run it again. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(tasks): sanitize rich text and extract mentions`.

### Task 5: List the board

**Files:**
- Create `backend/src/domain/tasks.rs` and `backend/src/api/tasks.rs`.
- Modify `backend/src/api/mod.rs`, `backend/src/app.rs` and `backend/src/openapi.rs`.
- Create `backend/tests/tasks.rs`.

**Interfaces:**
- Produces `pub struct BoardFilter { mentioned: Option<Uuid>, author: Option<Uuid>, meeting: Option<Uuid> }`.
- Produces `pub async fn list(db, team_id, actor: Uuid, filter: BoardFilter) -> Result<Vec<TaskView>, AppError>`.
  `TaskView` is the task, its author `user::Model`, its mentioned `Vec<user::Model>`, and
  `can_edit: bool`.
- Produces `pub async fn insert(db, team_id, author_id, status, raw_content) -> Result<task::Model, AppError>`.
  It is used by tests and by the dev route, and puts the task at the end of the column.
- Produces `TaskDto` (camelCase): `id`, `status`, `position`, `contentHtml`, `author: PersonDto`,
  `mentions: Vec<PersonDto>`, `meetingId: Option<Uuid>`, `createdAt`, `canEdit`.
- Produces the handler `api::tasks::list_tasks`, `GET /api/v1/teams/{team_id}/tasks` with the
  query `mentioned`, `author` and `meeting` (`IntoParams`).

- [ ] **Step 1: Failing tests** in `tasks.rs`. Seed through `domain::tasks::insert`, and copy
  the `create_team` and `join` helpers from `teams.rs`:
  - `lists_tasks_by_column_and_position`: three tasks inserted in Active come back in insertion
    order, with `canEdit` true for the author and the admin and false for another member.
  - `filters_by_mention_author_and_meeting`: each filter narrows the list; `meeting` with a
    random uuid gives `[]`.
  - `non_members_get_404`: `team_not_found`.
  - `deleting_the_team_deletes_its_tasks`.
- [ ] **Step 2:** Run `cargo test --test tasks`. Expected: FAIL.
- [ ] **Step 3:** Implement it. Check membership with `teams::require_member`. Load authors and
  mentions in bulk (`teams::users_by_id` plus one `task_mentions` query), with no per-task
  queries.
- [ ] **Step 4:** Run it again. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(tasks): list the task board with filters`.

### Task 6: Move a task

**Files:** Modify `backend/src/domain/tasks.rs`, `backend/src/api/tasks.rs`, `app.rs`,
`openapi.rs` and `backend/tests/tasks.rs`.

**Interfaces:**
- Produces `pub async fn move_task(db, task_id, actor, status: TaskStatus, after: Option<Uuid>, before: Option<Uuid>) -> Result<TaskView, AppError>`.
- Produces the handler `api::tasks::move_task`, `PUT /api/v1/tasks/{task_id}/position`. The body
  is `MoveTaskRequest { status, after_id: Option<Uuid>, before_id: Option<Uuid> }` (camelCase),
  and it returns 200 `TaskDto`.
- Semantics:
  - The task ends up after `afterId` and before `beforeId`. With neither, it goes to the end of
    the column (after the current maximum position, the task itself excluded).
  - A neighbour that isn't in the same team and target column, or is the task itself → 400
    `invalid_position`.
  - `key_between` returning `None`, or a unique violation on update → 409 `stale_position`.
  - An unknown task, or one in a team the caller isn't in → 404 `task_not_found`.

- [ ] **Step 1: Failing tests:**
  - `moves_to_the_end_of_another_column`.
  - `moves_between_neighbours`.
  - `moves_between_non_adjacent_neighbours`.
  - `rejects_foreign_neighbours`: a task from another team, and one from another column.
  - `stale_neighbours_conflict`: `afterId` placed after `beforeId` → 409.
  - `concurrent_identical_keys_conflict`: move T1 with `afterId=A, beforeId=B`, then move T2
    with the same neighbours. The server doesn't check adjacency, so it computes the same key,
    and the second move gets 409 `stale_position`.
  - `any_member_can_move`.
  - `strangers_get_404`.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement it in one transaction. Lock the task row
  (`SELECT … FOR UPDATE`) and load the neighbours' positions. Map the unique-violation
  `DbErr` (SQLSTATE `23505`) to `Conflict("stale_position")`.
- [ ] **Step 4:** Run them again. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(tasks): move tasks between and within columns`.

### Task 7: Edit and delete

**Files:** Modify `backend/src/domain/tasks.rs`, `backend/src/api/tasks.rs`, `app.rs`,
`openapi.rs` and `backend/tests/tasks.rs`.

**Interfaces:**
- Produces `pub async fn edit(db, task_id, actor, raw: &str) -> Result<TaskView, AppError>`.
- Produces `pub async fn delete(db, task_id, actor) -> Result<(), AppError>`.
- Produces the handlers:
  - `api::tasks::update_task`: `PATCH /api/v1/tasks/{task_id}`, taking
    `UpdateTaskRequest { content: String }` and returning 200 `TaskDto`.
  - `api::tasks::delete_task`: `DELETE /api/v1/tasks/{task_id}` → 204.
- A member who isn't the author or an admin gets 403 `forbidden`. Non-members get 404
  `task_not_found`.

- [ ] **Step 1: Failing tests:**
  - `author_and_admin_can_edit_others_cannot` (200/200/403).
  - `edit_sanitizes_content`.
  - `edit_keeps_only_member_mentions`: a member is mentioned, then a stranger. The response's
    `mentions` lists only the member, and the `mentioned=<stranger>` filter gives `[]`.
  - `blank_content_is_rejected` (400 `invalid_task_content`).
  - `author_and_admin_can_delete_others_cannot`.
  - `delete_removes_mentions`.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement it. `edit` sanitizes, updates `content`/`updated_at`, then deletes
  and re-inserts the mentions, keeping only member ids, all in one transaction.
- [ ] **Step 4:** Run them again. Expected: PASS.
- [ ] **Step 5:** Run `npm run api:generate` and re-export `TaskDto`, `TaskStatus`,
  `MoveTaskRequest` and `UpdateTaskRequest` from `frontend/src/api/client.ts`. Then run
  `npm run lint`. Expected: PASS.
- [ ] **Step 6:** Commit: `feat(tasks): edit and delete tasks`.

### Task 8: Dev-only seeding route

**Files:** Create `backend/src/api/dev.rs`. Modify `backend/src/api/mod.rs`,
`backend/src/app.rs` and `backend/tests/tasks.rs`.

**Interfaces:**
- Produces `POST /api/v1/dev/teams/{team_id}/tasks`. The body is
  `{ status, content, authorId?: Uuid }`, where `authorId` defaults to the caller and must be a
  member. It returns 201 `TaskDto`. The caller must be a member.
- In `app.rs`, it is routed only `if state.config.seed_dev_users`. It isn't in `openapi.rs`.
  The module doc says it is removed once Discuss creates tasks.

- [ ] **Step 1: Failing tests:**
  - `dev_route_seeds_tasks_when_enabled`: the default test config has seeding on → 201.
  - `dev_route_is_absent_when_disabled`: `TestApp::with_config(|c| c.seed_dev_users = false)`
    → 404.
- [ ] **Step 2:** Run them, implement, then run them again. Expected: FAIL, then PASS.
- [ ] **Step 3:** Commit: `test(tasks): dev-only route to seed tasks`.

### Task 9: Rich text components

**Files:**
- Modify `frontend/package.json` and the lockfile. Install, exact-pinned:
  `@tiptap/vue-3 @tiptap/pm @tiptap/starter-kit @tiptap/extension-mention @tiptap/suggestion dompurify vue-draggable-plus`,
  plus `@tiptap/extension-link` only if the installed StarterKit doesn't bundle Link.
- Create `frontend/src/components/{RichText.vue,RichTextEditor.vue}`.
- Test: create `frontend/tests/RichText.test.ts`.

**Interfaces:**
- `RichText`:
  - Props `{ html: string }`. It renders `DOMPurify.sanitize(html, { ALLOWED_TAGS: ['p','br','strong','em','a','span'], ALLOWED_ATTR: ['href','rel','data-type','data-id','data-label'] })`
    with `v-html`. If the `vue/no-v-html` rule is on, disable it for that one line with a
    comment naming the sanitizer.
  - Mention spans are styled as chips: `--accent-soft` / `--accent-text`, radius 6, padding
    `0 6px`, weight 600.
- `RichTextEditor`:
  - Props `{ modelValue: string; members: PersonDto[]; label: string }`. It emits
    `update:modelValue` with `editor.getHTML()`.
  - A toolbar with Bold, Italic and Link buttons (`aria-pressed`). Link opens a small inline URL
    field, with no browser dialogs.
  - Mention suggestions: the `@` trigger shows a component-rendered `role="listbox"` of matching
    members (ArrowUp, ArrowDown, Enter, Escape). The inserted node renders
    `<span data-type="mention" data-id data-label>@Name</span>`.
  - The editable area has `aria-label` set to `label`.

- [ ] **Step 1: Failing tests:**
  - `RichText renders mention chips and links`.
  - `RichText drops script and handlers`.
  - `RichTextEditor emits HTML when typing`: set the content through the editor's
    `commands.setContent` and expect the emit.
  - `RichTextEditor inserts a mention from the suggestion list`.
- [ ] **Step 2:** Run `npx vitest run tests/RichText.test.ts`. Expected: FAIL.
- [ ] **Step 3:** Implement it.
- [ ] **Step 4:** Run it again. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(tasks): rich text viewer and editor with mentions`.

### Task 10: Tasks store

**Files:** Create `frontend/src/stores/tasks.ts` and `frontend/tests/tasksStore.test.ts`.

**Interfaces:**
- Produces `useTasksStore()`, which returns:
  - `tasks: Ref<TaskDto[]>`, `loading`, `error: Ref<string|null>`;
  - `columns: ComputedRef<Record<TaskStatus, TaskDto[]>>`, sorted by `position` with plain
    string comparison;
  - `load(teamId: string, filter: { mentioned?: string; author?: string; meeting?: string }): Promise<void>`;
  - `move(taskId: string, status: TaskStatus, toIndex: number): Promise<void>`;
  - `update(taskId: string, contentHtml: string): Promise<void>`;
  - `remove(taskId: string): Promise<void>`.
- `move` works like this:
  1. It reads the neighbours from `columns[status]` without the moved task, at `toIndex - 1`
     (`afterId`) and `toIndex` (`beforeId`).
  2. It places the task optimistically: it changes the status and sets a temporary position
     taken from the neighbours, so the computed column shows it at `toIndex`.
  3. It calls the API. On 200, it replaces the task with the response.
  4. On an error, it restores the snapshot and sets `error` to the code. On 409 it also calls
     `load` again with the last filter.
- `toIndex === columns[status].length` and no neighbours means it sends neither id (end of
  column).

- [ ] **Step 1: Failing tests:**
  - `groups and sorts by column`.
  - `move sends the visible neighbours` (asserts the request body).
  - `move to an empty column sends no neighbours`.
  - `move rolls back and reloads on 409`.
  - `remove deletes and drops the task`.
  - `update replaces the task with the server copy`.
- [ ] **Step 2:** Run them, implement, then run them again. Expected: FAIL, then PASS.
- [ ] **Step 3:** Commit: `feat(tasks): tasks store with optimistic moves`.

### Task 11: Board view, columns, cards, drag and drop

**Files:**
- Create `frontend/src/features/tasks/{TasksBoardView.vue,TaskColumn.vue,TaskCard.vue}`.
- Modify `frontend/src/router/index.ts` (`team-tasks` → `TasksBoardView`, and `ComingSoonView`
  loses its `tasks` variant and its i18n key) and `en.ts`.
- Test: create `frontend/tests/TasksBoardView.test.ts`.

**Interfaces:**
- `TasksBoardView`:
  - The header is "Tasks" plus the subtitle "Created during the Discuss phase of a meeting.
    Drag cards between columns, or use Move to… from the keyboard."
  - It loads the store on mount and on `window` `focus`, removing the listener on unmount.
  - Below the header, a grid of 4 `TaskColumn`s, which scrolls horizontally inside its own box
    at narrow widths.
  - When the board has no tasks at all and no filter is active, an empty state reads "No tasks
    yet. Tasks are created in the Discuss phase of a meeting."
- `TaskColumn`:
  - It is a `<section :aria-label="column name">`. The header has the name and a count pill.
  - `VueDraggable` with `group="tasks"`, `:data-status`, and `@add` / `@update` mapped to
    `store.move(id, status, newIndex)`.
- `TaskCard`:
  - It is an `<article>` showing the date (`10 Nov`, through
    `Intl.DateTimeFormat('en-GB', { day: 'numeric', month: 'short' })`). A meeting link appears
    only when `meetingId` is set; it targets the meeting route, which doesn't exist yet, so
    render the text without a link.
  - `RichText` shows the content, and `UserAvatar` plus the name show the author.
  - It has an actions button (`aria-label="Task actions"`), and the menu comes from Task 13.

- [ ] **Step 1: Failing tests:**
  - `renders four columns with counts in order`.
  - `renders a card's date, content with mention chip and author`.
  - `shows the empty state when there are no tasks`.
  - `reloads on window focus`.
  - `dropping a card calls PUT position`: trigger the column component's `add` event with
    `{ newIndex, item: { dataset: { id } } }`.
- [ ] **Step 2:** Run them, implement, then run them again. Expected: FAIL, then PASS.
- [ ] **Step 3:** Commit: `feat(tasks): task board with drag and drop`.

### Task 12: Filters

**Files:** Create `frontend/src/features/tasks/TaskFilters.vue` and modify
`TasksBoardView.vue`, `en.ts` and `frontend/tests/TasksBoardView.test.ts`.

**Interfaces:**
- The filter state lives in the route query `mentioned`, `author` and `meeting`, updated with
  `router.replace`. The view watches the query and calls `store.load`.
- Controls, in the design's order:
  - "My tasks" is a toggle button whose `aria-pressed` is `mentioned === me`.
  - "Mentioned" and "Author" are labelled `<select>`s of team members (`api.listMembers`). Their
    first options are "anyone".
  - "Meeting" is rendered only when some loaded task has a `meetingId`.
  - "Clear filters" appears only when a filter is set.

- [ ] **Step 1: Failing tests:**
  - `My tasks filters by mentioned=me and reflects in the URL`.
  - `author select sends author=`.
  - `Clear filters resets the query`.
  - `meeting filter hidden without meetings`.
- [ ] **Step 2:** Run them, implement, then run them again. Expected: FAIL, then PASS.
- [ ] **Step 3:** Commit: `feat(tasks): board filters`.

### Task 13: Card menu — Move to…, Edit, Delete

**Files:**
- Create `frontend/src/features/tasks/{TaskMenu.vue,EditTaskDialog.vue}`.
- Modify `TaskCard.vue`, `en.ts` and `frontend/tests/TasksBoardView.test.ts`.

**Interfaces:**
- `TaskMenu`:
  - It is a `role="menu"` positioned with `usePopover`, with Edit and Delete only when
    `task.canEdit`.
  - "Move to…" opens a submenu listing the other three columns. Choosing one calls
    `store.move(id, status, columns[status].length)`.
  - Focus returns to the actions button when it closes.
- `EditTaskDialog`: an `AppDialog` titled "Edit task", containing `RichTextEditor` (label "Task",
  members from the team) and Cancel/Save buttons. Save calls `store.update`, and errors show
  `errors.<code>`.
- Delete goes through `ConfirmDialog`: "Delete this task?" / "This can't be undone." / "Delete".

- [ ] **Step 1: Failing tests:**
  - `menu shows Edit and Delete only when canEdit`.
  - `Move to Done appends to Done`: the request has `{status:'done'}` and no neighbours when Done
    is empty, or `afterId` set to the last Done task otherwise.
  - `edit saves new content`.
  - `delete asks for confirmation then removes the card`.
- [ ] **Step 2:** Run them, implement, then run them again. Expected: FAIL, then PASS.
- [ ] **Step 3:** Run `npm run test`. Expected: PASS (everything).
- [ ] **Step 4:** Commit: `feat(tasks): task actions menu`.

### Task 14: Visual pass and end-to-end test

**Files:**
- `frontend/src/features/tasks/*.vue` styles.
- Create `tests/e2e/tasks/tasks.spec.ts` and `tests/e2e/pages/TasksPage.ts`.
- Modify `tests/e2e/api.ts`.

**Interfaces:**
- Produces `seedTask(login, teamId, { status, content, authorId? }): Promise<{ id: string }>` in
  `api.ts`, which calls the dev route.
- Produces `TasksPage`:
  - `column(name)` and `card(text)`;
  - `dragTo(cardText, columnName)`, which uses `locator.dragTo`; if SortableJS doesn't react,
    it falls back to `page.mouse` down, then a move in 10 steps, then up;
  - `moveWithKeyboard(cardText, columnName)` and `filterMine()`;
  - `edit(cardText, newText)` and `delete(cardText)`.

- [ ] **Step 1:** Compare with `Tasks-L.dc.html` and `Tasks-D.dc.html`: spacing, the pill and
  tag tokens (Stuck uses `--col-delta-*`, Done uses `--col-plus-*`, Active uses `--accent-soft`,
  Future uses `--well`), and the card shadow. Check there is no page-level horizontal scroll at
  390 px.
- [ ] **Step 2:** Write `tasks.spec.ts`:
  1. `resetAccount('alice')` and `resetAccount('bob')`.
  2. Alice creates the team "Tasks E2E", and bob joins through the link (copy the pattern from
     `teams.spec.ts`).
  3. Seed three tasks: one by alice mentioning bob (in Active), one by bob (in Future), and one
     by alice (in Stuck).
  4. As alice, open Tasks. All four columns are visible with the counts 1/1/1/0.
  5. Drag the Active card to Done, then reload: it is still in Done.
  6. Use Move to… on the Stuck card to put it in Active.
  7. Turn on My tasks as bob in a second context: only the card mentioning bob is shown.
  8. As alice, edit her card to add text, then reload and check the text persists.
  9. Delete it and check it is gone.
- [ ] **Step 3:** Run `scripts/build.sh && scripts/restart.sh`, then
  `scripts/test-playwright.sh https://$(scripts/dev-host.sh)`. Expected: the whole suite passes.
- [ ] **Step 4:** Commit: `test(tasks): end-to-end happy path`.

### Task 15: Verification

- [ ] Run `npm run lint`. Expected: exit 0.
- [ ] Run `npm run test`. Expected: all tests pass.
- [ ] Run `npm run api:generate`, then `git status`. Expected: a clean tree.
