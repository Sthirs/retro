# ADR-0007: Minikube, Telepresence and dex as the development environment

- **Status:** 🟡 proposed
- **Date:** 2026-11-12
- **Deciders:** Retro maintainers

## Context

Developers need the whole system (ingress, backend, frontend, database, identity
provider) running locally, with fast iteration on one component at a time.

## Decision

We will support exactly one development workflow:

- **Minikube** with the ingress addon; the application host is
  `retro.<minikube ip>.nip.io` (no `/etc/hosts` edits).
- Images are built inside Minikube (`minikube image build`) with a fixed `:dev` tag
  and deployed with the chart and `values-dev.yaml`. `scripts/dev.sh` does
  everything in one command.
- **dex** is the development identity provider, with static users that mirror the
  backend's seeded accounts (`alice`, `bob`, password `password`). The same people
  can sign in with a password or through dex, which exercises account linking.
- **Telepresence** intercepts the backend or frontend so either can run locally with
  hot reload (`cargo watch`, `vite`) against the in-cluster services.

## Alternatives considered

- **docker compose** — a second deployment description that drifts from the chart.
- **Keycloak** — heavier and slower to start; dex covers what development needs
  (standard OIDC, static users, password connector).
- **Port-forwarding** — breaks the single-host cookie and issuer model.

## Sources / Prior art

- Spec §18.
- Telepresence: <https://telepresence.io/docs/>; nip.io: <https://nip.io/>.

## Consequences

- Positive: development matches production topology, including cookies and SSO.
- Negative: requires Minikube resources (about 4 CPUs, 6 GB).
- Negative: a fixed `:dev` tag means a rebuilt image needs a pod restart
  (`scripts/restart.sh`).
