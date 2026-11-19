# Build context: backend/
FROM rust:1.99-slim-trixie@sha256:f6c4a35d21dbb3c89d7a8d056c193e94fc09037f2b58dd858b09c39a47dd02d7 AS build
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
