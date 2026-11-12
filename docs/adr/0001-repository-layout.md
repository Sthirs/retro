# ADR-0001: Repository layout and tooling

- **Status:** 🟡 proposed
- **Date:** 2026-11-12
- **Deciders:** Retro maintainers

## Context

Retro has a Rust backend, a Vue frontend, a Helm chart and end-to-end tests
(spec §2, §15–§19). They change together: an API change touches the backend, the
generated TypeScript client and often a screen. Contributors and coding agents need
one place to find code, docs and commands.

## Decision

We will keep everything in one repository with these top-level directories:

- `backend/` — a Cargo workspace: the `retro-backend` crate and the `migration` crate.
- `frontend/` — the Vue application, the only npm workspace.
- `deploy/helm/retro/` — the Helm chart and its `values-dev.yaml` overlay.
- `scripts/` — Bash scripts for the development cluster and tests.
- `tests/e2e/` — Playwright specs and page objects.
- `docs/` — specification, development guide and ADRs.

npm scripts in the root `package.json` are the single command entry point
(`npm run lint`, `npm run test`, `npm run api:generate`, `npm run cluster:*`), wrapping
cargo for the backend. Node is pinned in `.nvmrc`, Rust in `backend/rust-toolchain.toml`.

## Alternatives considered

- **Separate repositories per component** — API changes would need coordinated
  releases and cross-repo PRs; the generated client would drift.
- **A task runner (make, just)** — an extra tool to install; npm is already required
  for the frontend and Playwright.
- **`apps/` + `helm/` layout** — equally valid; the spec already names
  `backend/`, `frontend/` and `deploy/helm/retro`, so we follow it.

## Sources / Prior art

- Spec §15.3, §16.2, §17.2 (paths).
- npm workspaces: <https://docs.npmjs.com/cli/using-npm/workspaces>.
- Cargo workspaces: <https://doc.rust-lang.org/cargo/reference/workspaces.html>.

## Consequences

- Positive: one PR covers API, client and UI; CI checks the whole system.
- Negative: CI runs Rust and Node toolchains; contributors install both.
- Follow-ups: a module size limit and repository-wide lint rules, if the codebase grows.
