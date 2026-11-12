#!/usr/bin/env bash
set -euo pipefail
# Runs Vite locally; HMR goes through the Minikube ingress host.

# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"
DEV_HOST="$("$REPO_ROOT/scripts/dev-host.sh")"

export RETRO_DEV_HOST="$DEV_HOST"
export FRONTEND_PORT="${FRONTEND_PORT:-5173}"
export VITE_HMR_CLIENT_PORT="${VITE_HMR_CLIENT_PORT:-80}"
export BACKEND_URL="${BACKEND_URL:-http://$DEV_HOST}"

cd "$REPO_ROOT"
npm run dev --workspace @retro/frontend -- --host 0.0.0.0 --port "$FRONTEND_PORT"
