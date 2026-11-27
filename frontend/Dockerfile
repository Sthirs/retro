# Build context: repository root (npm workspace lockfile lives there).
FROM node:26.13.1-alpine@sha256:6bd9ab43c0ec0190ca7b2d193906cb8b7edf6e7f4afa43ddeccd6452710bffb5 AS build
WORKDIR /app

ARG APP_VERSION=dev
ENV VITE_APP_VERSION=$APP_VERSION

COPY package.json package-lock.json ./
COPY frontend/package.json ./frontend/package.json
RUN npm ci --workspace @retro/frontend --include-workspace-root=false --ignore-scripts

COPY frontend ./frontend
RUN npm run build --workspace @retro/frontend \
  && find frontend/dist -type f \( -name '*.js' -o -name '*.css' -o -name '*.html' -o -name '*.svg' \) \
     -exec gzip -9 -k -n {} +

FROM nginx:1.31.10-alpine@sha256:7fc0fb4af7bf006aeb31aea88069976060f8f0a0a86b667126742cc17f4e31ae
# Run as non-root on a read-only root filesystem: drop the `user` directive and
# keep the pid file in /tmp (temp paths are set in nginx.conf).
RUN sed -i -e '/^user /d' -e 's,^pid .*,pid /tmp/nginx.pid;,' /etc/nginx/nginx.conf
COPY frontend/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/frontend/dist /usr/share/nginx/html
# Numeric uid (nginx) so the kubelet can verify runAsNonRoot.
USER 101
EXPOSE 8080
