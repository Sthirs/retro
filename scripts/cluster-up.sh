#!/usr/bin/env bash
set -euo pipefail
# Starts Minikube with the ingress addon.

minikube status >/dev/null 2>&1 || minikube start --cpus="${MINIKUBE_CPUS:-4}" --memory="${MINIKUBE_MEMORY:-6g}"
minikube addons enable ingress
kubectl -n ingress-nginx wait --for=condition=ready pod \
  -l app.kubernetes.io/component=controller --timeout=180s
