#!/usr/bin/env bash
set -euo pipefail
# Checks the development tools and installs project dependencies.

cd "$(dirname "$0")/.."

missing=0
for tool in node npm cargo minikube helm kubectl; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'missing: %s\n' "$tool" >&2
    missing=1
  fi
done
if ! command -v docker >/dev/null 2>&1 && ! command -v podman >/dev/null 2>&1; then
  printf 'missing: docker or podman (backend integration tests)\n' >&2
  missing=1
fi
for optional in telepresence cargo-watch; do
  command -v "$optional" >/dev/null 2>&1 || printf 'optional, not found: %s\n' "$optional" >&2
done
[ "$missing" -eq 0 ] || exit 1

expected_node="$(cat .nvmrc)"
if [ "$(node --version)" != "v$expected_node" ]; then
  printf 'warning: node %s found, .nvmrc expects v%s (run: nvm install && nvm use)\n' "$(node --version)" "$expected_node" >&2
fi

npm ci
cargo fetch --manifest-path backend/Cargo.toml
echo "Setup complete. Next: scripts/dev.sh"
