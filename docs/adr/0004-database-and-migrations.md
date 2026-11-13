# ADR-0004: PostgreSQL schema ownership and migrations

- **Status:** 🟡 proposed
- **Date:** 2026-11-12
- **Deciders:** Retro maintainers

## Context

The backend owns the schema (spec §13, §16.1). Several replicas may start at the same
time, and rolling upgrades run old and new pods side by side.

## Decision

We will manage the schema with **sea-orm-migration** in the `backend/migration` crate,
written as plain SQL per migration, and apply migrations:

- at startup (`RUN_MIGRATIONS=startup`), inside one transaction holding
  `pg_advisory_xact_lock`, so only one replica migrates at a time; or
- through a per-release Kubernetes Job running `retro-backend migrate`
  (Helm `backend.runMigrations: job`, the production default).

`/readyz` reports ready only when every migration known to the binary is applied.
Migrations applied by a newer release are ignored, so pods of the previous release
stay ready during a rolling upgrade. Migrations must therefore be backward compatible
with the previous release (expand, then contract).

Emails and usernames are stored lower-cased (enforced by `CHECK` constraints) so
uniqueness and look-ups are case-insensitive without extensions.

## Alternatives considered

- **Helm pre-install/pre-upgrade hook Job** — hooks run before the release's
  ConfigMap and Secrets exist, so the Job would need its own copy of the configuration.
- **Automatic schema sync** — no review of schema changes, unsafe in production.
- **citext** — needs an extension; lower-casing on write is enough.

## Sources / Prior art

- Spec §13, §16.1, §17.2.
- PostgreSQL advisory locks: <https://www.postgresql.org/docs/current/explicit-locking.html#ADVISORY-LOCKS>.
- Expand/contract migrations: <https://martinfowler.com/bliki/ParallelChange.html>.

## Consequences

- Positive: safe concurrent startups; zero-downtime rolling upgrades when migrations
  are backward compatible.
- Negative: every migration must be written to tolerate the previous release.
- Follow-ups: seed data for greetings and icebreaker questions (spec §13).
