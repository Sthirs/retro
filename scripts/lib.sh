#!/usr/bin/env bash
# Shared settings for the dev scripts. Source it; do not run it.
# shellcheck disable=SC2034

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAMESPACE="${RETRO_NAMESPACE:-retro}"
RELEASE="${RETRO_RELEASE:-retro}"
CHART="$REPO_ROOT/deploy/helm/retro"
