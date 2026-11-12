#!/usr/bin/env bash
set -euo pipefail
# Routes in-cluster frontend traffic to Vite running locally on :5173.
# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"

telepresence intercept "$RELEASE-frontend" --port 5173:http --mount false
