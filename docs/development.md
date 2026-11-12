# Development

There is exactly one supported way to develop Retro: the whole application runs in
**Minikube**, deployed with the same Helm chart as production plus
`deploy/helm/retro/values-dev.yaml`. **Telepresence** lets you run the backend or
the frontend on your machine with hot reload while they talk to the rest of the
cluster. The background is in [ADR-0007](adr/0007-development-environment.md).

```text
browser ──▶ ingress  retro.<minikube ip>.nip.io
              ├── /api  → backend   (Rust, :3000; metrics :9090)
              ├── /dex  → dex       (OIDC provider, dev only)
              └── /     → frontend  (nginx serving the Vue build, :8080)
backend ──▶ postgres (dev only, ephemeral)
```

## Prerequisites

| Tool | Version | Notes |
| --- | --- | --- |
| Node.js + npm | see `.nvmrc` | `nvm install && nvm use` |
| Rust | see `backend/rust-toolchain.toml` | installed by rustup automatically |
| Docker or Podman | recent | backend integration tests (testcontainers) |
| Minikube | recent | about 4 CPUs and 6 GB of memory |
| Helm | 3.x or 4.x | |
| kubectl | matching the cluster | |
| Telepresence | 2.x | optional: local backend or frontend |
| cargo-watch | any | optional: backend hot reload (`cargo install cargo-watch`) |

`scripts/setup.sh` checks these and installs the project dependencies.

## First-time setup

```bash
nvm install && nvm use
scripts/setup.sh        # checks tools, npm ci, cargo fetch
scripts/dev.sh          # Minikube + ingress, build images, deploy, print the URL
```

Open the printed URL (`http://retro.<minikube ip>.nip.io`). `scripts/dev-host.sh`
prints the host at any time.

### Seeded users

In dev mode the backend seeds `alice` and `bob` at startup, and dex has the same
people, so each can sign in **with a password or through single sign-on**. `carol`
exists only in dex: she can sign in **only through single sign-on**, and her Retro
account is created on her first sign-in (`OIDC_AUTO_REGISTER`).

| Username | Email | Name | Password | Sign-in methods |
| --- | --- | --- | --- | --- |
| `alice` | `alice@retro.local` | Alice Rossi | `password` | password, SSO |
| `bob` | `bob@retro.local` | Bob Bianchi | `password` | password, SSO |
| — | `carol@retro.local` | Carol Verdi | `password` (in dex) | SSO only |

On the first SSO sign-in, the backend links the dex identity to the existing account
with the same verified email ([ADR-0005](adr/0005-authentication.md)). The seed runs
on every backend start and resets names and passwords; it is refused when
`APP_ENV=production`.

## Daily development

### Everything in the cluster

```bash
scripts/dev.sh          # idempotent: rebuild, redeploy, restart
```

### Backend only (local, hot reload)

```bash
scripts/telepresence-connect.sh
scripts/telepresence-intercept-backend.sh   # cluster traffic for the backend → localhost:3000
scripts/dev-backend.sh                      # cargo watch, configured like values-dev.yaml
```

### Frontend only (local, hot reload)

```bash
scripts/telepresence-connect.sh
scripts/telepresence-intercept-frontend.sh  # cluster traffic for the frontend → localhost:5173
scripts/dev-frontend.sh                     # vite; HMR through the ingress host
```

### Both

Run both intercepts, then `npm run dev` (backend and frontend side by side). Stop with
`scripts/telepresence-leave.sh`.

### Testing a rebuilt image

Images use a fixed `:dev` tag, so a Helm upgrade does **not** restart pods after a
rebuild. Either run `scripts/dev.sh`, or:

```bash
scripts/build.sh        # minikube image build, fails loudly on build errors
scripts/deploy.sh       # only needed when the chart or values changed
scripts/restart.sh      # rollout restart backend and frontend
```

### Things to know

- `scripts/db-reset.sh` wipes the database (the dev Postgres is ephemeral) and
  restarts the backend, which migrates and re-seeds.
- `scripts/cluster-down.sh` removes the release and namespace;
  `scripts/cluster-down.sh --delete` also deletes the Minikube cluster.
- Restarting dex rotates its signing keys; the backend re-fetches them on the next
  SSO sign-in.
- The API contract is generated: after changing an endpoint or DTO run
  `npm run api:generate` and commit `frontend/openapi.json` and
  `frontend/src/api/schema.d.ts`.
- The scripts read `RETRO_NAMESPACE`, `RETRO_RELEASE` and `RETRO_DEV_HOST` to override
  the defaults (`retro`, `retro`, `retro.<minikube ip>.nip.io`).

## Configuration

The backend reads environment variables only; invalid values stop it at startup.
The chart sets them from `values.yaml` (ConfigMap and Secrets).

### Server

| Variable | Default | Description |
| --- | --- | --- |
| `APP_ENV` | `production` | `development` or `production` |
| `PUBLIC_URL` | — (required) | External base URL, e.g. `https://retro.example.com` |
| `BIND_ADDR` | `0.0.0.0:3000` | HTTP listener |
| `METRICS_ADDR` | `0.0.0.0:9090` | Prometheus listener (`/metrics`) |
| `RUST_LOG` | `info,sqlx=warn,sea_orm=warn` | Log filter |
| `LOG_FORMAT` | JSON | `pretty` for human-readable logs |

### Database

| Variable | Default | Description |
| --- | --- | --- |
| `DATABASE_URL` | — | Full URL; wins over the variables below |
| `DB_HOST` / `DB_PORT` | — / `5432` | |
| `DB_NAME` / `DB_USER` / `DB_PASSWORD` | `retro` / — / — | |
| `DB_SSLMODE` | `prefer` | Chart default: `require` |
| `RUN_MIGRATIONS` | `startup` | `startup` (advisory-locked), `job` or `none` |

### Authentication

| Variable | Default | Description |
| --- | --- | --- |
| `JWT_SECRET` | — (required, ≥ 32 chars) | Signs access tokens |
| `STATE_SECRET` | — (required, ≥ 32 chars) | Encrypts the SSO state cookie |
| `ACCESS_TOKEN_TTL_SECONDS` | `900` | Access token lifetime |
| `REFRESH_TOKEN_TTL_SECONDS` | `2592000` | "Keep me signed in" session (30 days) |
| `SESSION_REFRESH_TTL_SECONDS` | `43200` | Browser-session sign-in (12 hours) |
| `REFRESH_REUSE_GRACE_SECONDS` | `10` | Window in which a rotated refresh token is still accepted (concurrent tabs) |
| `COOKIE_SECURE` | `true` | `false` only for plain-HTTP development |
| `LOGIN_RATE_LIMIT` | `10` | Failed sign-ins per login name and per IP per 5 minutes |
| `AUTH_LOCAL_LOGIN_ENABLED` | `true` | Show the username/password form |
| `AUTH_PASSWORD_RESET_URL` | — | "Forgot password?" link target; hidden when unset |
| `OIDC_ENABLED` | `false` | Single sign-on |
| `OIDC_ISSUER_URL` | — | Provider issuer |
| `OIDC_CLIENT_ID` / `OIDC_CLIENT_SECRET` | — | Confidential client credentials |
| `OIDC_SCOPES` | `openid email profile` | |
| `OIDC_BUTTON_TEXT` | `Continue with single sign-on` | |
| `OIDC_AUTO_REGISTER` | `true` | Create accounts for unknown SSO users |
| `SEED_DEV_USERS` | `false` | Seed `alice` and `bob`; refused in production |

The OIDC redirect URI to register with the provider is
`<PUBLIC_URL>/api/v1/auth/oidc/callback`.

## Testing

| Suite | Command | Needs |
| --- | --- | --- |
| Lint (all) | `npm run lint` | — |
| Backend unit + integration | `npm run test:backend` | Docker/Podman, or `TEST_DATABASE_URL=postgres://user:pass@host:5432/postgres` |
| Frontend unit | `npm run test:frontend` | — |
| End-to-end | `scripts/test-playwright.sh http://$(scripts/dev-host.sh)` | the cluster running the current code |

Backend integration tests create a fresh database per test. With Podman, export
`DOCKER_HOST=unix://$XDG_RUNTIME_DIR/podman/podman.sock` and enable the Podman socket.

End-to-end tests run against the deployed application (no `webServer`): rebuild and
restart first (`scripts/dev.sh`). Install the browser once with
`npx playwright install chromium`. Reports land in `playwright-report/`.

## Command reference

| Command | What it does |
| --- | --- |
| `scripts/dev.sh` (`npm run dev:up`) | Cluster, images, deploy, restart, print URL |
| `scripts/cluster-up.sh` | Start Minikube and the ingress addon |
| `scripts/build.sh` | Build `retro/backend:dev` and `retro/frontend:dev` in Minikube |
| `scripts/deploy.sh` | `helm upgrade --install` with `values-dev.yaml` |
| `scripts/restart.sh` | Restart backend and frontend pods |
| `scripts/db-reset.sh` | Wipe and re-seed the dev database |
| `scripts/cluster-down.sh [--delete]` | Remove the release (and the cluster) |
| `scripts/dev-backend.sh` / `scripts/dev-frontend.sh` | Run a component locally |
| `scripts/telepresence-*.sh` | Connect, intercept, leave |
| `npm run api:generate` | Regenerate `openapi.json` and the TypeScript client |
| `npm run lint` / `npm run test` | Lint and test everything |

## Repository structure

```text
backend/                Cargo workspace
  src/                  api/ auth/ domain/ entities/ config.rs seed.rs …
  migration/            sea-orm-migration crate (SQL migrations)
  tests/                integration tests (in-process router + PostgreSQL)
frontend/
  src/                  api/ auth/ components/ features/ stores/ router/ styles/ i18n/
  tests/                Vitest
deploy/helm/retro/      chart, values.yaml, values-dev.yaml
scripts/                dev, cluster, telepresence and test scripts
tests/e2e/              Playwright specs and page objects
docs/                   specifications.md, development.md, adr/
```
