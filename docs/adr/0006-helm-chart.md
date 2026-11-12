# ADR-0006: One Helm chart with dev-only services

- **Status:** 🟡 proposed
- **Date:** 2026-11-12
- **Deciders:** Retro maintainers

## Context

Spec §17.2 asks for one chart with separate frontend and backend deployments, an
ingress, configuration and secrets, and no database in production. Development needs
PostgreSQL and an identity provider in the cluster (spec §18).

## Decision

We will ship one chart, `deploy/helm/retro`, that deploys the frontend, the backend,
the ingress (`/api` → backend, `/` → frontend), a ConfigMap, Secrets (or
`existingSecret` references), an optional migration Job and optional HPAs.

Development-only PostgreSQL and dex templates render **only when `devMode.enabled`
and their own toggle are both true**. Both toggles default to false, so the production
contract is unchanged. `values-dev.yaml` is the development overlay. Dex is served on
the application host under `/dex`, so the browser and the backend use the same issuer
URL. Outside dev mode, pods run as non-root with a read-only root filesystem and all
capabilities dropped.

## Alternatives considered

- **Separate dev chart for PostgreSQL and dex** — two releases to keep in step, and
  the ingress path for dex would cross charts.
- **Upstream dex and PostgreSQL charts as dependencies** — heavier, slower to start,
  and more configuration than two small dev-only templates.

## Sources / Prior art

- Spec §17, §18.
- Helm chart best practices: <https://helm.sh/docs/chart_best_practices/>.
- dex static clients and passwords: <https://dexidp.io/docs/connectors/local/>.

## Consequences

- Positive: development runs the same chart as production.
- Negative: the chart contains templates that production never renders.
- Follow-ups: NetworkPolicies and PodDisruptionBudgets for production.
