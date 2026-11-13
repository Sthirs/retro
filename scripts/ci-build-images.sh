#!/usr/bin/env bash
set -uo pipefail
# CI only: builds the backend and frontend images in parallel with Buildx,
# using the GitHub Actions layer cache, and writes them as tarballs for
# `minikube image load`. Meant to run in the background while the cluster
# starts; writes <out-dir>/exit-code when both builds are finished.
#
# Requires a docker-container Buildx builder and the ACTIONS_RUNTIME_TOKEN /
# ACTIONS_RESULTS_URL variables (crazy-max/ghaction-github-runtime).
#
# Usage: scripts/ci-build-images.sh <out-dir>

cd "$(dirname "$0")/.." || exit 1
out="${1:?usage: $0 <out-dir>}"
mkdir -p "$out"
# Never leave the waiting step hanging: report failure if the script dies early.
trap '[ -f "$out/exit-code" ] || echo 1 > "$out/exit-code"' EXIT

build() {
  local app="$1" context="$2" dockerfile="$3"
  docker buildx build "$context" \
    --file "$dockerfile" \
    --tag "retro/$app:dev" \
    --cache-from "type=gha,scope=e2e-$app" \
    --cache-to "type=gha,mode=max,scope=e2e-$app" \
    --output "type=docker,dest=$out/$app.tar" \
    > "$out/$app.log" 2>&1
}

build backend backend backend/Dockerfile &
backend=$!
build frontend . frontend/Dockerfile &
frontend=$!

status=0
wait "$backend" || status=1
wait "$frontend" || status=1
echo "$status" > "$out/exit-code"
