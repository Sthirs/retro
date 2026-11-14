# ADR-0010: Rich text is stored as server-sanitized HTML

- **Status:** 🟡 proposed
- **Date:** 2026-11-14
- **Deciders:** Retro maintainers

## Context

Tasks carry rich text with @mentions (spec §10.1), and reflections will carry rich text
too (spec §9.2, §9.5). The spec leaves the storage format open ("sanitized HTML (or editor
JSON)") and requires the server to sanitize it against an allow-list. Mentions must be
queryable so that mentioned users can filter "my tasks". Content is rendered on the client
and must be safe against script injection even if stored data is tampered with.

## Decision

We will store rich text as HTML that the server sanitizes with **ammonia** against a fixed
allow-list before saving it. Mentions are `span[data-type="mention"]` elements carrying a
`data-id` (and a display label); the server derives the `task_mentions` rows from them,
keeping only users who are current team members. The client sanitizes the stored HTML again
with **DOMPurify** before rendering it with `v-html`. Tasks use this format first;
reflections reuse it later. The allow-list starts small (paragraphs, line breaks, bold,
italic, links and mention spans) and grows with the features that need more.

## Alternatives considered

- **ProseMirror JSON** — every client would need a renderer for the editor schema, and
  server-side search and mention extraction would have to walk a JSON tree instead of
  working on markup the server already controls.
- **Markdown** — mentions and images need custom syntax on top of it, and it still has to
  be rendered to HTML and sanitized on the client.

## Sources / Prior art

- Spec §9.2 (reflection content), §9.5 (task rich text), §10.1 (task card and mentions),
  §16.1 (ammonia on the server) and §15.1 (Tiptap editor, DOMPurify on the client).
- ammonia, whitelist-based HTML sanitizer: <https://docs.rs/ammonia>.
- DOMPurify: <https://github.com/cure53/DOMPurify>.
- Tiptap `Mention` extension, which renders mentions as `span[data-type="mention"]`:
  <https://tiptap.dev/docs/editor/extensions/nodes/mention>.

## Consequences

- Positive: the server owns the format, so sanitizing, mention extraction and future search
  work on one representation; clients render it without an editor schema.
- Positive: two sanitization layers (server on write, client on render) limit the damage of
  a bug in either.
- Negative: HTML is looser than a document tree, so the allow-list must be maintained, and
  a new editor feature (images, lists, strikethrough) requires extending both the server
  allow-list and the client configuration.
- Negative: mention labels are stored in the HTML and can go stale when a user is renamed.
- Follow-ups: extending the allow-list for pasted images and the other reflection
  formatting (spec §9.2) when reflections adopt this format.
