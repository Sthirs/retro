#!/usr/bin/env bash
set -euo pipefail
# Resets the dev database: the Postgres pod is ephemeral, so restarting it
# wipes the data; the backend then migrates and re-seeds at startup.

# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"

kubectl -n "$NAMESPACE" rollout restart "deployment/$RELEASE-postgres"
kubectl -n "$NAMESPACE" rollout status "deployment/$RELEASE-postgres" --timeout=120s
kubectl -n "$NAMESPACE" rollout restart "deployment/$RELEASE-backend"
kubectl -n "$NAMESPACE" rollout status "deployment/$RELEASE-backend" --timeout=180s
echo "Database reset; dev users re-seeded."
