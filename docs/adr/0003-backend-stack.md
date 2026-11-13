# ADR-0003: Backend stack

- **Status:** 🟡 proposed
- **Date:** 2026-11-12
- **Deciders:** Retro maintainers

## Context

Spec §16.1 fixes Rust with Tokio, Axum, SeaORM, utoipa and tracing. This ADR records
the concrete choices made for the first implementation and how configuration and
operations work.

## Decision

We will build the backend as a single binary, `retro-backend`, with subcommands
`serve` (default), `migrate` and `openapi`, using:

- **Axum 0.8** + tower-http (request IDs, tracing, response headers).
- **SeaORM 2** with PostgreSQL, plus a separate `migration` crate (ADR-0004).
- **utoipa** for the OpenAPI 3.1 document, served at `/api/v1/openapi.json`.
- **Errors** as RFC 9457 problem details with a stable `code` field.
- **Configuration** from environment variables only, parsed and validated at startup
  (`config.rs`); an invalid value stops the process with a clear message.
- **Observability**: JSON logs (tracing-subscriber), Prometheus metrics on a separate
  port (`metrics-exporter-prometheus`), `/healthz` and `/readyz`.
- **Auth crates**: `argon2`, `jsonwebtoken`, `openidconnect`, `axum-extra`
  cookies (ADR-0005).

The released version is injected at build time through the `APP_VERSION` build
argument; the crate version stays `0.0.0` (ADR-0009).

## Alternatives considered

- **figment / config crates** — layered configuration is unnecessary when every value
  comes from the environment; a small explicit reader gives precise error messages
  and is trivially testable.
- **sqlx without an ORM** — the spec chooses SeaORM; raw SQL is still used where clearer
  (migrations, readiness).
- **Separate migrate binary** — one image with subcommands keeps the Helm Job and the
  pods on the exact same build.

## Sources / Prior art

- Spec §14, §16, §20.
- RFC 9457 Problem Details: <https://www.rfc-editor.org/rfc/rfc9457>.
- Axum: <https://docs.rs/axum>; SeaORM: <https://www.sea-ql.org/SeaORM/>.

## Consequences

- Positive: one artifact for serving, migrating and generating the API contract.
- Negative: SeaORM 2 and utoipa 6 are young major versions; upgrades may need code changes.
- Follow-ups: WebSocket hub and PostgreSQL LISTEN/NOTIFY fan-out (spec §11).
