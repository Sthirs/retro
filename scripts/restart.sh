#!/usr/bin/env bash
set -euo pipefail
# Restarts the app pods so they pick up freshly built :dev images.

# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"

for component in backend frontend; do
  kubectl -n "$NAMESPACE" rollout restart "deployment/$RELEASE-$component"
done
for component in backend frontend; do
  kubectl -n "$NAMESPACE" rollout status "deployment/$RELEASE-$component" --timeout=180s
done
