# Build context: backend/
FROM rust:1.97-slim-trixie@sha256:8e8cf8f7fd54a2d23d5a743b3a03f56e26b6c774276c33fa0595111704ebb15c AS build
WORKDIR /src

# Dependencies first, compiled against stub crate roots: this layer is reused
# (locally and through the CI layer cache) until Cargo.toml or Cargo.lock
# change, so a source change only recompiles the two workspace crates.
COPY Cargo.toml Cargo.lock ./
COPY migration/Cargo.toml ./migration/
RUN mkdir -p src migration/src \
    && echo 'fn main() {}' > src/main.rs \
    && touch src/lib.rs migration/src/lib.rs \
    && cargo build --release --locked --bin retro-backend \
    && rm -rf src migration/src

ARG APP_VERSION=dev
ENV APP_VERSION=$APP_VERSION

COPY migration ./migration
COPY src ./src

# COPY keeps the build context's mtimes, which can predate the stub build:
# touch the crate roots so cargo rebuilds both workspace crates.
RUN touch src/main.rs src/lib.rs migration/src/lib.rs \
    && cargo build --release --locked --bin retro-backend \
    && cp target/release/retro-backend /retro-backend

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:cc66d222f0a7ec996b78a99fe76a969bc80b539a5016ad866c14d1949bc6d0ab
COPY --from=build /retro-backend /usr/local/bin/retro-backend
USER nonroot
EXPOSE 3000 9090
ENTRYPOINT ["/usr/local/bin/retro-backend"]
CMD ["serve"]
