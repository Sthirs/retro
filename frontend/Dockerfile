# Build context: repository root (npm workspace lockfile lives there).
FROM node:26.13.1-alpine3.23@sha256:39d8c143e60e99ae98f014bebea1e72fc96d3c9bdf9504bc3a27af5c05bfa39f AS build
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

FROM nginx:1.31.9-alpine@sha256:fb4658584bd9a916ffb6f6e365f97c25cfdb2923801f551ea08356e695f14ff7
# Run as non-root on a read-only root filesystem: drop the `user` directive and
# keep the pid file in /tmp (temp paths are set in nginx.conf).
RUN sed -i -e '/^user /d' -e 's,^pid .*,pid /tmp/nginx.pid;,' /etc/nginx/nginx.conf
COPY frontend/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/frontend/dist /usr/share/nginx/html
# Numeric uid (nginx) so the kubelet can verify runAsNonRoot.
USER 101
EXPOSE 8080
