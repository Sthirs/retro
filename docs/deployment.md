# Deployment

How to run Retro in production on Kubernetes with the Helm chart. For a local
development cluster, see the [development guide](development.md) instead.

The product-level requirements are in the
[specification, §17](specifications.md#17-deployment); the chart's design is in
[ADR-0006](adr/0006-helm-chart.md).

## What gets deployed

```text
                    ┌─────────── Ingress (TLS) ───────────┐
                    │  /      → frontend                  │
                    │  /api   → backend (REST + WebSocket)│
                    └──────┬───────────────────┬──────────┘
                           │                   │
                 ┌─────────▼────────┐ ┌────────▼─────────┐     ┌──────────────┐
                 │ frontend (nginx) │ │  backend (Rust)  │ ──▶ │  PostgreSQL  │
                 │   static SPA     │ │ API, WS, metrics │     │ (yours, not  │
                 └──────────────────┘ └────────▲─────────┘     │ in the chart)│
                                               │               └──────────────┘
                                   migration Job (per release)
```

| Component | Kind | Notes |
| --- | --- | --- |
| Frontend | `Deployment` + `Service` (port 8080) | Unprivileged nginx serving the SPA. Optional HPA. |
| Backend | `Deployment` + `Service` (port 3000, metrics 9090) | Probes on `/healthz` and `/readyz`. Optional HPA. |
| Migrations | `Job` | One per release revision; runs `migrate`. Pods become ready once their migrations are applied. |
| Ingress | `Ingress` | `/` → frontend, `/api` → backend, long timeouts for WebSockets, 6 MB bodies for image uploads. |
| Configuration | `ConfigMap` + `Secret`s | Auth secrets and the database password, or your own existing secrets. |

**Not deployed:** PostgreSQL. Provision it separately (a managed service or a
dedicated cluster) and point the chart at it.

## Prerequisites

- A Kubernetes cluster and Helm 3.8+ (for OCI chart registries).
- An ingress controller. The chart's defaults and annotations target
  [ingress-nginx](https://kubernetes.github.io/ingress-nginx/); with another
  controller, set `ingress.className` and the equivalent WebSocket timeout and
  body-size annotations in `ingress.annotations`.
- A TLS certificate for your host, as a `kubernetes.io/tls` secret (for example
  issued by cert-manager).
- PostgreSQL 18 (the version the project is developed and tested against), with
  an empty database and a user that owns it.
- Optional: an OIDC provider for single sign-on.

## Artifacts

Each GitHub release publishes multi-arch (`linux/amd64`, `linux/arm64`) images and
the chart to GitHub Container Registry:

| Artifact | Location |
| --- | --- |
| Frontend image | `ghcr.io/sthirs/retro/frontend:<version>` |
| Backend image | `ghcr.io/sthirs/retro/backend:<version>` |
| Helm chart | `oci://ghcr.io/sthirs/retro/charts/retro` |

Images are also tagged `<major>.<minor>` and `sha-<commit>`. The chart deploys
the images of its own `appVersion` unless you set `frontend.image.tag` /
`backend.image.tag`.

## 1. Prepare the database

```sql
CREATE ROLE retro LOGIN PASSWORD '<db-password>';
CREATE DATABASE retro OWNER retro;
```

The migration Job creates and upgrades the schema; nothing else is needed.

## 2. Create the secrets

Keep secrets out of your values file by creating them up front. The JWT and
state secrets must be at least 32 characters.

```bash
kubectl create namespace retro

kubectl -n retro create secret generic retro-auth \
  --from-literal=JWT_SECRET="$(openssl rand -base64 48)" \
  --from-literal=STATE_SECRET="$(openssl rand -base64 48)" \
  --from-literal=OIDC_CLIENT_SECRET='<client-secret>'   # only with single sign-on

kubectl -n retro create secret generic retro-database \
  --from-literal=DB_PASSWORD='<db-password>'
```

Alternatively, leave `auth.existingSecret` / `database.existingSecret` empty and
pass `auth.jwtSecret`, `auth.stateSecret`, `oidc.clientSecret` and
`database.password` at install time; the chart then creates the secrets.

## 3. Write your values

A minimal `values-prod.yaml`:

```yaml
ingress:
  className: nginx
  host: retro.example.com
  tls:
    enabled: true
    secretName: retro-tls

database:
  host: postgres.example.com
  name: retro
  user: retro
  sslMode: require
  existingSecret: retro-database

auth:
  existingSecret: retro-auth

backend:
  resources:
    requests: { cpu: 100m, memory: 128Mi }
    limits: { memory: 512Mi }
frontend:
  resources:
    requests: { cpu: 25m, memory: 32Mi }
    limits: { memory: 64Mi }
```

### Single sign-on

```yaml
oidc:
  enabled: true
  issuerUrl: https://idp.example.com/realms/retro
  clientId: retro
  # Extra root CAs for a provider behind a private CA (secret with `ca.crt`).
  caSecret: ""
  # Create an account on the first SSO sign-in of an unknown user.
  autoRegister: true
  buttonText: Continue with single sign-on
```

Register Retro with your provider as a **confidential client** using the
Authorization Code flow, with this redirect URI:

```text
https://retro.example.com/api/v1/auth/oidc/callback
```

Retro requests the scopes `openid email profile` and requires `email_verified`.
To offer single sign-on only, set `auth.localLogin: false`.

## 4. Install

```bash
helm install retro oci://ghcr.io/sthirs/retro/charts/retro \
  --version <version> \
  --namespace retro \
  -f values-prod.yaml
```

Helm prints the public URL when it is done. Follow the rollout with:

```bash
kubectl -n retro get pods,jobs
kubectl -n retro rollout status deploy/retro-backend
```

## 5. Upgrade

```bash
helm upgrade retro oci://ghcr.io/sthirs/retro/charts/retro \
  --version <new-version> \
  --namespace retro \
  -f values-prod.yaml
```

Each revision runs a new migration Job. Backend pods report ready on `/readyz`
only once the database has the migrations they need, so old pods keep serving
until the new ones are ready. Back up the database before upgrading: migrations
only move forward, and `helm rollback` does not undo them.

## Configuration reference

The full list, with comments, is in
[`deploy/helm/retro/values.yaml`](../deploy/helm/retro/values.yaml). The values
you are most likely to change:

| Value | Default | Description |
| --- | --- | --- |
| `publicUrl` | `http(s)://<ingress.host>` | External base URL, when it differs from the ingress host |
| `ingress.enabled` / `className` / `host` | `true` / `nginx` / `retro.example.com` | Ingress routing |
| `ingress.tls.enabled` / `secretName` | `false` / — | TLS at the ingress |
| `frontend.replicas`, `backend.replicas` | `2` | Replicas when autoscaling is off |
| `*.autoscaling.enabled` | `false` | HPA on CPU (2–5 replicas, 80 %) |
| `backend.runMigrations` | `job` | `job`, `startup` (each pod migrates, under an advisory lock) or `none` |
| `backend.logLevel` | `info` | `RUST_LOG` filter |
| `backend.env` | `{}` | Extra backend environment variables |
| `auth.localLogin` | `true` | Username and password sign-in |
| `auth.passwordResetUrl` | — | Target of "Forgot password?"; hidden when empty |
| `auth.accessTokenTtlSeconds` | `900` | Access token lifetime |
| `auth.refreshTokenTtlSeconds` | `2592000` | "Keep me signed in" session (30 days) |
| `auth.sessionRefreshTtlSeconds` | `43200` | Browser-session lifetime (12 hours) |
| `database.host` / `port` / `name` / `user` / `sslMode` | — / `5432` / `retro` / `retro` / `require` | PostgreSQL connection |
| `imagePullSecrets` | `[]` | For a private mirror of the images |

Never set `devMode.enabled` in production: it deploys throwaway PostgreSQL and
dex instances, relaxes pod hardening and seeds demo accounts. The backend refuses
to seed when it runs in production.

## Operations

- **Health:** `GET /healthz` (alive) and `GET /readyz` (database reachable and
  migrated) on the backend's HTTP port.
- **Metrics:** Prometheus metrics (HTTP, WebSocket connections, events) on
  `/metrics`, port 9090 of the backend pods, not exposed through the ingress.
- **Logs:** structured JSON on stdout.
- **Security:** pods run as non-root with a read-only root filesystem, no
  privilege escalation and all capabilities dropped. TLS terminates at the
  ingress.
- **Scaling:** both services are stateless and scale horizontally. Real-time
  events fan out through PostgreSQL `LISTEN/NOTIFY`, so every backend replica
  sees every event without extra infrastructure.
- **Backups:** everything, uploaded images included, lives in PostgreSQL; back
  up the database and you have backed up Retro.
