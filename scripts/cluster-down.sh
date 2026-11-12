#!/usr/bin/env bash
set -euo pipefail
# Removes the release and namespace. Pass --delete to delete the Minikube cluster too.

# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"

helm uninstall "$RELEASE" -n "$NAMESPACE" --wait 2>/dev/null || true
kubectl delete namespace "$NAMESPACE" --ignore-not-found
if [ "${1:-}" = "--delete" ]; then
  minikube delete
fi
