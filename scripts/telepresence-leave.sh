#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"

telepresence leave "$RELEASE-backend" 2>/dev/null || true
telepresence leave "$RELEASE-frontend" 2>/dev/null || true
