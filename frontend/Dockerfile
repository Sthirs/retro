# Build context: repository root (npm workspace lockfile lives there).
FROM node:26.13.0-alpine3.23@sha256:44a3247313f9bc069a4ec25d01b66254ed7dc4d0408727d8720dc827f0753423 AS build
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

FROM nginxinc/nginx-unprivileged:1.31.2-alpine3.23@sha256:6320020c7da8714feab524e02c08c5a1958675c4e68700e93a2fd8970b065786
COPY frontend/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/frontend/dist /usr/share/nginx/html
USER nginx
EXPOSE 8080
