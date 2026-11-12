# ADR-0002: Frontend stack

- **Status:** 🟡 proposed
- **Date:** 2026-11-12
- **Deciders:** Retro maintainers

## Context

Spec §15.1 fixes Vue 3, TypeScript, Vite, Vue Router and Pinia, and leaves the
component library and styling open. The design (`.design/brief.md`) defines its own
tokens, components and a single typeface, light and dark themes, and a CSP that
forbids third-party fonts (spec §20).

## Decision

We will build the frontend with Vue 3 (`<script setup>`, TypeScript strict), Vite,
Vue Router, Pinia and vue-i18n, styled with **plain CSS custom properties** generated
from the design tokens (`src/styles/tokens.css`) and scoped component styles, with
**no component library**. The API client is `openapi-fetch` typed by
`openapi-typescript` from the backend's OpenAPI document. Plus Jakarta Sans is
self-hosted through `@fontsource-variable/plus-jakarta-sans`.

The access token is kept in memory only (Pinia store); the session survives reloads
through the backend's httpOnly refresh cookie (ADR-0005).

## Alternatives considered

- **PrimeVue / Naive UI** — their visual language and theming fight the bespoke design;
  overriding them costs more than writing the few primitives we need.
- **Tailwind CSS** — workable, but the design is token-first; CSS variables map 1:1 to
  the brief and switch theme without a build step.
- **axios** — `openapi-fetch` gives end-to-end types from the OpenAPI document with a
  smaller footprint.
- **Token in `localStorage`** — readable by any injected script; the spec requires memory.

## Sources / Prior art

- Spec §3, §15, §20; `.design/brief.md` (tokens, typography, theming).
- openapi-fetch: <https://openapi-ts.dev/openapi-fetch/>.
- OWASP HTML5 Security Cheat Sheet, local storage:
  <https://cheatsheetseries.owasp.org/cheatsheets/HTML5_Security_Cheat_Sheet.html#local-storage>.

## Consequences

- Positive: the UI matches the artboards exactly; theme switching is pure CSS.
- Negative: we own accessibility and behaviour of every primitive (dialogs, menus).
- Negative: TypeScript stays on the 6.x line (aliased) until vue-tsc supports the
  native TypeScript 7 compiler.
- Follow-ups: rich-text editor (Tiptap) and drag-and-drop library choices when the
  meeting phases are built.
