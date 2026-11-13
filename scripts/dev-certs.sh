#!/usr/bin/env bash
set -euo pipefail
# Issues a locally trusted (mkcert) certificate for the dev host and stores it,
# with the mkcert root CA, as the secrets values-dev.yaml references:
#   retro-dev-tls  the ingress certificate
#   retro-dev-ca   the root CA, so the backend trusts dex behind the ingress
# Run `mkcert -install` once (scripts/setup.sh) so browsers trust it too.

# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"
DEV_HOST="$("$REPO_ROOT/scripts/dev-host.sh")"

if ! command -v mkcert >/dev/null 2>&1; then
  echo "mkcert not found: install it (https://github.com/FiloSottile/mkcert), then run scripts/setup.sh" >&2
  exit 1
fi

CERT_DIR="$REPO_ROOT/.dev-certs"
CERT="$CERT_DIR/$DEV_HOST.pem"
KEY="$CERT_DIR/$DEV_HOST-key.pem"
mkdir -p "$CERT_DIR"
# The host follows the Minikube IP, so certificates are kept per host.
if [ ! -f "$CERT" ] || [ ! -f "$KEY" ]; then
  mkcert -cert-file "$CERT" -key-file "$KEY" "$DEV_HOST"
fi

kubectl create namespace "$NAMESPACE" --dry-run=client -o yaml | kubectl apply -f - >/dev/null
kubectl -n "$NAMESPACE" create secret tls retro-dev-tls --cert="$CERT" --key="$KEY" \
  --dry-run=client -o yaml | kubectl apply -f - >/dev/null
kubectl -n "$NAMESPACE" create secret generic retro-dev-ca \
  --from-file=ca.crt="$(mkcert -CAROOT)/rootCA.pem" \
  --dry-run=client -o yaml | kubectl apply -f - >/dev/null
echo "TLS certificate for https://$DEV_HOST is in place"
