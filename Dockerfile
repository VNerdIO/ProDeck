# ProDeck: one container serving the REST API and the WASM web client.
# Build: docker build -t prodeck .

ARG RUST_VERSION=1
ARG TRUNK_VERSION=0.22.1

FROM rust:${RUST_VERSION}-bookworm AS chef
RUN cargo install cargo-chef --locked && rustup target add wasm32-unknown-unknown
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG TRUNK_VERSION
RUN curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz" \
    | tar -xz -C /usr/local/bin trunk
COPY --from=planner /src/recipe.json recipe.json
# Dependency layers: cached until Cargo.toml/Cargo.lock change.
RUN cargo chef cook --release -p server --recipe-path recipe.json \
 && cargo chef cook --release -p web --target wasm32-unknown-unknown --recipe-path recipe.json
COPY . .
RUN cargo build --release -p server --locked \
 && cd crates/web && trunk build --release --locked

FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=builder /src/target/release/prodeck /app/prodeck
COPY --from=builder /src/dist /app/dist
ENV STATIC_DIR=/app/dist PORT=8080
EXPOSE 8080
USER nonroot
ENTRYPOINT ["/app/prodeck"]
