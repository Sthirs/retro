#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"

telepresence helm install 2>/dev/null || telepresence helm upgrade || true
telepresence connect --namespace "$NAMESPACE"
