#!/usr/bin/env bash
set -euo pipefail
# Prints the dev ingress host: retro.<minikube ip>.nip.io (override with RETRO_DEV_HOST).

if [ -n "${RETRO_DEV_HOST:-}" ]; then
  printf '%s\n' "$RETRO_DEV_HOST"
  exit 0
fi
printf 'retro.%s.nip.io\n' "$(minikube ip)"
