FROM lukemathwalker/cargo-chef:latest-rust-alpine AS chef

RUN apk add --no-cache musl-dev

WORKDIR /app

FROM chef AS planner

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

ARG TARGETARCH

RUN case "$TARGETARCH" in \
      amd64) echo "x86_64-unknown-linux-musl" > /tmp/rust_target ;; \
      arm64) echo "aarch64-unknown-linux-musl" > /tmp/rust_target ;; \
      *) echo "Unsupported architecture: $TARGETARCH" && exit 1 ;; \
    esac && \
    rustup target add $(cat /tmp/rust_target)

COPY --from=planner /app/recipe.json recipe.json

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release -p pipedash-push-relay --recipe-path recipe.json --target $(cat /tmp/rust_target)

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release -p pipedash-push-relay --target $(cat /tmp/rust_target)

RUN strip target/$(cat /tmp/rust_target)/release/pipedash-push-relay && \
    cp target/$(cat /tmp/rust_target)/release/pipedash-push-relay /pipedash-push-relay && \
    mkdir /data

FROM scratch

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /pipedash-push-relay /pipedash-push-relay
COPY --from=builder --chown=65532:65532 /data /data

ENV RELAY_BIND_ADDR=0.0.0.0:8080
ENV RELAY_DATABASE_PATH=/data/relay.db

VOLUME /data
EXPOSE 8080

USER 65532:65532

ENTRYPOINT ["/pipedash-push-relay"]
