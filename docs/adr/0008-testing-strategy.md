# ADR-0008: Testing strategy

- **Status:** 🟡 proposed
- **Date:** 2026-11-12
- **Deciders:** Retro maintainers

## Context

The spec (§19) asks for unit, integration and end-to-end tests in CI. Auth and
permissions bugs are security bugs, so endpoints must be tested against a real
database.

## Decision

We will test at three levels:

- **Backend unit tests** in `#[cfg(test)]` modules for pure logic (configuration,
  passwords, tokens, rate limiting, redirect sanitising).
- **Backend integration tests** in `backend/tests/`, driving the real Axum router
  in-process (`tower::ServiceExt::oneshot`) against a fresh PostgreSQL per test, from
  testcontainers or from `TEST_DATABASE_URL`.
- **Frontend unit tests** with Vitest, happy-dom and `@vue/test-utils`, mounting views
  with the real router and stores and a stubbed `fetch`.
- **End-to-end tests** with Playwright against the application deployed in Minikube,
  including real SSO through dex. Every user-facing feature has at least one.

## Alternatives considered

- **Integration tests against a deployed backend over HTTP** — slower feedback and
  shared state between tests; in-process tests with a database per test are isolated.
- **Mock OIDC server in e2e** — dex is already the development provider and is real
  OIDC.

## Sources / Prior art

- Spec §19, §20.
- testcontainers-rs: <https://rust.testcontainers.org/>.
- Playwright: <https://playwright.dev/docs/best-practices>.

## Consequences

- Positive: endpoint behaviour is verified against PostgreSQL on every run.
- Negative: backend tests need Docker or Podman locally.
- Follow-ups: WebSocket integration tests; accessibility checks in e2e.
