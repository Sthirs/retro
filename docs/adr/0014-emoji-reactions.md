# ADR-0014: Emoji reactions are Unicode emoji, validated on the server

- **Status:** 🟡 proposed
- **Date:** 2026-11-18
- **Deciders:** Retro maintainers

## Context

In Discuss, participants react to reflections with emoji ([`docs/specifications.md`](../specifications.md)
§9.5): several different emoji per card, one of each per person, counts per emoji, and reacting is
a toggle. The data model stores `reactions(reflection_id, user_id, emoji, created_at)` with
`PK(reflection_id, user_id, emoji)` (§13), and the API takes the emoji as a path segment
(`PUT/DELETE /reflections/{rid}/reactions/{emoji}`, §14.4).

Forces:

- The value comes from the client and is shown to everyone, so the server must decide what is an
  emoji: free text would let anyone put words, HTML or very long strings on a card.
- The same emoji can be written several ways (with or without the U+FE0F variation selector, e.g.
  "❤" and "❤️"): stored as sent, they would count as two different reactions.
- The ReactionPicker artboard has a search with `:shortcodes:` and category tabs (Smileys,
  People & gestures, Nature, Food, Objects, Symbols), plus "Recently used"; the user asked for the
  first row to be the viewer's most used emoji, kept in the browser.
- Browsers render only the emoji versions their fonts know; newer ones show as boxes.

## Decision

We will accept any single Unicode emoji (including skin-tone and ZWJ sequences) as a reaction,
validated on the server against the Unicode emoji list of the Rust `emojis` crate and stored in its
fully qualified form; the client's picker catalogue comes from the `unicode-emoji-json` package,
lazy-loaded and limited to emoji versions browsers render, and the viewer's usage statistics are
kept in `localStorage` per user.

- `emojis::get` maps minimally qualified and unqualified forms to the fully qualified emoji, which
  is what we store and return. Anything else is `400 invalid_emoji`.
- The picker loads `data-by-group.json` with a dynamic `import()`, so the meeting room does not
  carry it until someone opens the picker. Unicode groups map onto the design's tabs (Travel &
  Places and Activities join Objects; Flags join Symbols). Emoji newer than version 15.0 are left
  out of the picker (the server still accepts them).
- `retro.emojiUsage.{userId}` holds `{ counts, recent }`: the first row of the picker is the most
  used emoji, padded with defaults; "Recently used" lists the latest ones. Nothing about usage is
  sent to the server.

## Alternatives considered

- **A fixed allow-list of a few dozen emoji, shared by client and server** — simple to validate,
  but the design has a full picker with search and categories, and the list would have to be kept
  in sync by hand in two languages.
- **Free text up to N characters** — no dependency, but lets non-emoji text onto cards and keeps
  duplicate spellings of the same emoji apart.
- **`node-emoji` (emojilib) on the client** — good shortcode search, but no categories for the
  design's tabs, and its data was last updated in 2024.
- **`emojibase-data`** — complete, with categories, but a much larger package for the same data.
- **Usage statistics on the server** — would follow the user across browsers, but adds a table and
  an endpoint for a convenience the user asked to keep in local storage.

## Sources / Prior art

- [`docs/specifications.md`](../specifications.md) §9.5, §13, §14.4.
- ReactionPicker and Discuss artboards (`.design/project/`).
- [ADR-0002](0002-frontend-stack.md) (frontend stack; libraries chosen as phases are built) and
  [ADR-0012](0012-ephemeral-client-messages-and-user-events.md) (user-targeted events, used for
  `reaction.mine_changed`).
- Unicode Technical Standard #51 (Unicode Emoji): fully qualified vs. unqualified sequences.
- [`emojis` crate](https://crates.io/crates/emojis) (Unicode 18.0 data, lookup of unqualified forms).
- [`unicode-emoji-json`](https://www.npmjs.com/package/unicode-emoji-json) (emoji by Unicode group,
  with names, slugs and versions).
- Slack and GitHub reactions: per-user toggle, counts per emoji, a "frequently used" row.

## Consequences

- Positive: any emoji works; duplicates by spelling are impossible; the picker matches the design;
  the room's bundle does not grow until the picker opens.
- Negative / trade-offs: two emoji datasets (Rust and npm) must be upgraded together for new
  Unicode versions; the most-used row does not follow the user to another browser.
- Follow-ups: skin-tone selection in the picker; whether the meeting summary shows reactions.
