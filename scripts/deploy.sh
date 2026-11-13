#!/usr/bin/env bash
set -euo pipefail
# Installs or upgrades the chart with the development overlay.

# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"
DEV_HOST="$("$REPO_ROOT/scripts/dev-host.sh")"

"$REPO_ROOT/scripts/dev-certs.sh"
helm upgrade --install "$RELEASE" "$CHART" \
  --namespace "$NAMESPACE" \
  --create-namespace \
  --values "$CHART/values-dev.yaml" \
  --set ingress.host="$DEV_HOST" \
  --wait \
  --timeout "${HELM_TIMEOUT:-5m}" \
  "$@"
