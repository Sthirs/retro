#!/usr/bin/env bash
set -euo pipefail
# Routes in-cluster backend traffic to a backend running locally on :3000.
# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"

telepresence intercept "$RELEASE-backend" --port 3000:http --mount false
