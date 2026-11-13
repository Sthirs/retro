#!/usr/bin/env bash
set -euo pipefail
# Runs the Playwright end-to-end suite against a deployed instance.
# Usage: scripts/test-playwright.sh https://retro.<minikube ip>.nip.io [playwright args…]

BASE_URL="${1:-${PLAYWRIGHT_BASE_URL:-}}"
if [ -z "$BASE_URL" ]; then
  printf 'A base URL is required, e.g.:\n  %s https://%s\n' "$0" "$("$(dirname "$0")/dev-host.sh" 2>/dev/null || echo 'retro.<minikube-ip>.nip.io')" >&2
  exit 1
fi
[ "$#" -gt 0 ] && shift
export PLAYWRIGHT_BASE_URL="${BASE_URL%/}"
cd "$(dirname "$0")/.."
npx playwright test --reporter=list "$@"
