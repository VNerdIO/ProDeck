# ProDeck: one container serving the REST API and the WASM web client.
#
# The build stages run on the builder's native platform and cross-compile to x86_64 Linux
# (Cloud Run's architecture), so building on an ARM Mac doesn't need CPU emulation.
#   podman build --platform linux/amd64 -t prodeck .
#   docker buildx build --platform linux/amd64 -t prodeck .

ARG RUST_VERSION=1
ARG TRUNK_VERSION=0.22.1

FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-bookworm AS chef
# x86_64-linux-gnu-gcc is the native compiler on amd64 builders and a cross compiler elsewhere.
RUN if [ "$(uname -m)" != "x86_64" ]; then \
      apt-get update && apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu libc6-dev-amd64-cross \
      && rm -rf /var/lib/apt/lists/*; \
    fi \
 && cargo install cargo-chef --locked \
 && rustup target add x86_64-unknown-linux-gnu wasm32-unknown-unknown
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
    CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc \
    AR_x86_64_unknown_linux_gnu=x86_64-linux-gnu-ar
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG TRUNK_VERSION
# Built from source: the prebuilt aarch64 Trunk binary needs a newer glibc than bookworm has.
RUN cargo install trunk --version "${TRUNK_VERSION}" --locked
COPY --from=planner /src/recipe.json recipe.json
# Dependency layers: cached until Cargo.toml/Cargo.lock change.
RUN cargo chef cook --release -p server --target x86_64-unknown-linux-gnu --recipe-path recipe.json \
 && cargo chef cook --release -p web --target wasm32-unknown-unknown --recipe-path recipe.json
COPY . .
RUN cargo build --release -p server --target x86_64-unknown-linux-gnu --locked \
 && cd crates/web && trunk build --release --locked

# Runtime: copy-only stage, so it never executes target-architecture code during the build.
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=builder /src/target/x86_64-unknown-linux-gnu/release/prodeck /app/prodeck
COPY --from=builder /src/dist /app/dist
ENV STATIC_DIR=/app/dist PORT=8080
EXPOSE 8080
USER nonroot
ENTRYPOINT ["/app/prodeck"]
