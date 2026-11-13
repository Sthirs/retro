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
- The dev host is served over **HTTPS** with a certificate from a local **mkcert**
  CA, trusted once per machine (`mkcert -install`, run by `scripts/setup.sh`).
  Cookies are therefore always `Secure`, as in production; the backend trusts the
  mkcert CA for its calls to dex through the ingress (`OIDC_CA_CERT_FILE`).
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
- **Plain HTTP with a `COOKIE_SECURE=false` switch** — makes a production cookie
  attribute configurable, so development no longer matches production; static
  analysis (CodeQL `rust/insecure-cookie`) rightly flags it.
- **Self-signed certificate without a local CA** — browser warnings on every visit,
  and every client would need certificate checks disabled.

## Sources / Prior art

- Spec §18.
- Telepresence: <https://telepresence.io/docs/>; nip.io: <https://nip.io/>.
- mkcert: <https://github.com/FiloSottile/mkcert>.

## Consequences

- Positive: development matches production topology, including cookies and SSO.
- Positive: HTTPS-only behaviour (`Secure` cookies, secure-context browser APIs) is
  exercised in development and end-to-end tests.
- Negative: requires Minikube resources (about 4 CPUs, 6 GB).
- Negative: mkcert is a prerequisite, and its root CA must be trusted on each
  developer machine.
- Negative: a fixed `:dev` tag means a rebuilt image needs a pod restart
  (`scripts/restart.sh`).
