#!/usr/bin/env bash
set -euo pipefail
# One command to a running dev environment: cluster, images, deploy, restart.

# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"
cd "$REPO_ROOT"

scripts/cluster-up.sh
scripts/build.sh
first_install=true
helm status "$RELEASE" -n "$NAMESPACE" >/dev/null 2>&1 && first_install=false
scripts/deploy.sh
# A fixed :dev tag does not change the pod spec: restart to run the new images.
if [ "$first_install" = false ]; then
  scripts/restart.sh
fi

DEV_HOST="$(scripts/dev-host.sh)"
cat <<INFO

Retro is running at http://$DEV_HOST

Seeded users (password "password"), for both password sign-in and single sign-on:
  alice  alice@retro.local  Alice Rossi
  bob    bob@retro.local    Bob Bianchi

End-to-end tests: scripts/test-playwright.sh http://$DEV_HOST
INFO
