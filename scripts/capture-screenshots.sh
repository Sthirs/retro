#!/usr/bin/env bash
set -euo pipefail
# Regenerates the README screenshots (docs/screenshots/) against a deployed
# instance. Mirrors scripts/test-playwright.sh, but points Playwright at
# playwright.screenshots.config.ts so the captures stay out of the e2e suite.
# Usage: scripts/capture-screenshots.sh https://retro.<minikube ip>.nip.io [playwright args…]

BASE_URL="${1:-${PLAYWRIGHT_BASE_URL:-}}"
if [ -z "$BASE_URL" ]; then
  printf 'A base URL is required, e.g.:\n  %s https://%s\n' "$0" "$("$(dirname "$0")/dev-host.sh" 2>/dev/null || echo 'retro.<minikube-ip>.nip.io')" >&2
  exit 1
fi
[ "$#" -gt 0 ] && shift
export PLAYWRIGHT_BASE_URL="${BASE_URL%/}"
cd "$(dirname "$0")/.."
npx playwright test --config=playwright.screenshots.config.ts --reporter=list "$@"
