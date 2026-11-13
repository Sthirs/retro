#!/usr/bin/env bash
set -euo pipefail
# Runs the backend locally with hot reload, against the in-cluster Postgres
# and dex (reachable through Telepresence). Values match values-dev.yaml so
# tokens issued here and by the pod are interchangeable.

# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"
DEV_HOST="$("$REPO_ROOT/scripts/dev-host.sh")"

export APP_ENV=development
export PUBLIC_URL="${PUBLIC_URL:-https://$DEV_HOST}"
export DATABASE_URL="${DATABASE_URL:-postgres://retro:retro@$RELEASE-postgres.$NAMESPACE:5432/retro?sslmode=disable}"
export JWT_SECRET="${JWT_SECRET:-dev-jwt-secret-dev-jwt-secret-dev-jwt-secret}"
export STATE_SECRET="${STATE_SECRET:-dev-state-secret-dev-state-secret-dev-state}"
export SEED_DEV_USERS=true
export LOGIN_RATE_LIMIT=1000
export OIDC_ENABLED=true
export OIDC_ISSUER_URL="${OIDC_ISSUER_URL:-https://$DEV_HOST/dex}"
OIDC_CA_CERT_FILE="${OIDC_CA_CERT_FILE:-$(mkcert -CAROOT)/rootCA.pem}"
export OIDC_CA_CERT_FILE
export OIDC_CLIENT_ID=retro
export OIDC_CLIENT_SECRET="${OIDC_CLIENT_SECRET:-retro-dev-secret}"
export LOG_FORMAT=pretty
export RUST_LOG="${RUST_LOG:-info,retro_backend=debug}"
export METRICS_ADDR="${METRICS_ADDR:-127.0.0.1:9090}"

cd "$REPO_ROOT/backend"
if command -v cargo-watch >/dev/null 2>&1; then
  exec cargo watch -x 'run -- serve'
fi
echo "cargo-watch not found (cargo install cargo-watch); running without hot reload" >&2
exec cargo run -- serve
