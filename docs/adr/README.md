# Architecture Decision Records

Architecture Decision Records (ADRs) capture decisions that are architectural,
cross-cutting, or hard to reverse. The rules in [`AGENTS.md` §3](../../AGENTS.md#3-adr-rules)
are binding.

## Process

1. Copy [`template.md`](template.md) to `NNNN-short-title.md` (next free number).
2. Fill in context, decision, alternatives, sources and consequences. Set status `proposed`.
3. A human reviewer accepts or rejects the ADR. **Only humans change the status.**
4. Add the ADR to the index below, with its status shown via the colored bullet from the legend.
5. A decision is changed by a *new* ADR that supersedes the old one — never by editing an
   accepted ADR.
6. **ADR numbers are permanent.** Never renumber, delete, or merge ADRs — other ADRs, commits
   (`Implements ADR-NNNN`), and code may reference a number. Superseded ADRs stay as historical
   record (status `superseded by ADR-NNNN`).
7. **Never reference an ADR number that does not exist yet.** Expected follow-up decisions are
   described by topic in the Consequences section; the number is cited once the file exists.

## Index

Legend: 🟢 accepted · 🟡 proposed · 🔴 rejected · ⚪ superseded

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-repository-layout.md) | Repository layout and tooling | 🟡 proposed |
| [0002](0002-frontend-stack.md) | Frontend stack | 🟡 proposed |
| [0003](0003-backend-stack.md) | Backend stack | 🟡 proposed |
| [0004](0004-database-and-migrations.md) | PostgreSQL schema ownership and migrations | 🟡 proposed |
| [0005](0005-authentication.md) | Backend-driven authentication: local accounts and OIDC | 🟡 proposed |
| [0006](0006-helm-chart.md) | One Helm chart with dev-only services | 🟡 proposed |
| [0007](0007-development-environment.md) | Minikube, Telepresence and dex as the development environment | 🟡 proposed |
| [0008](0008-testing-strategy.md) | Testing strategy | 🟡 proposed |
| [0009](0009-ci-release-and-versioning.md) | CI, releases and versioning | 🟡 proposed |
