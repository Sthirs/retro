#!/usr/bin/env bash
set -euo pipefail
# Builds the backend and frontend images inside Minikube (tag :dev).
#
# `minikube image build` can exit 0 when the build fails, so its output is
# checked for errors explicitly.

cd "$(dirname "$0")/.."

build() {
  local tag="$1"
  shift
  echo "=== Building $tag ==="
  local log
  log="$(mktemp)"
  minikube image build -t "$tag" "$@" 2>&1 | tee "$log"
  if grep -qE '^ERROR|failed to (build|solve)' "$log"; then
    rm -f "$log"
    echo "build of $tag failed" >&2
    exit 1
  fi
  rm -f "$log"
}

build retro/backend:dev -f Dockerfile backend
build retro/frontend:dev -f frontend/Dockerfile .
