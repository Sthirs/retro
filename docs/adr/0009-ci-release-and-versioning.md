# ADR-0009: CI, releases and versioning

- **Status:** 🟡 proposed
- **Date:** 2026-11-12
- **Deciders:** Retro maintainers

## Context

Spec §19 requires lint, test and build on every push to main and every PR, images
pushed only from main or tags, and checks for the chart and the generated API client.

## Decision

We will use GitHub Actions:

- `tests.yaml` runs on PRs and main: backend fmt, clippy and tests; frontend lint,
  type check, unit tests, build and a check that the generated API client is up to
  date; `helm lint`/`helm template` for production and dev values; and an e2e job that
  starts Minikube, builds the images, deploys with `values-dev.yaml` and runs Playwright.
- **Release Please** maintains a release PR from Conventional Commits. One version
  covers the whole repository: it is written to the root and frontend `package.json`
  and to `Chart.yaml` (`version` and `appVersion`).
- `release.yaml` runs when a release is published: multi-arch images
  (`linux/amd64`, `linux/arm64`) for backend and frontend are pushed to
  `ghcr.io/<owner>/retro/{backend,frontend}`, then the chart is pushed as an OCI artifact.
- The backend crate version stays `0.0.0`; the binary reports `APP_VERSION`, passed as a
  Docker build argument, so `Cargo.lock` never changes on release.
- **Renovate** updates dependencies weekly (grouped minor/patch), security fixes at
  any time. Actions and base images are pinned by digest.

## Alternatives considered

- **Release Please `rust` release type** — would bump `Cargo.toml` and `Cargo.lock`
  for a version nobody consumes as a crate.
- **Pushing images on every main commit** — the spec allows it; we start with
  release-only images and can add `main` tags later.

## Sources / Prior art

- Spec §17.1, §19.
- Release Please: <https://github.com/googleapis/release-please>.
- Conventional Commits: <https://www.conventionalcommits.org/>.

## Consequences

- Positive: versions and changelog come from commit messages; one version for all
  artifacts.
- Negative: the e2e job takes several minutes (Minikube boot and image builds).
- Follow-ups: image signing and SBOMs.
