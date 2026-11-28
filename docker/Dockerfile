FROM oven/bun:1.4.5@sha256:3aa040e5445efa09d75bb428a64ba50db13e2b3ac9b6b0c7f145615f62b2ee18 AS frontend-builder

WORKDIR /app

COPY package.json bun.lock ./
COPY patches ./patches
RUN --mount=type=cache,target=/root/.bun/install/cache \
    bun install --frozen-lockfile

COPY . .
ENV VITE_API_URL=/api/v1
RUN bun run build

FROM lukemathwalker/cargo-chef:0.1.78-rust-1.100.0-alpine@sha256:3dc65f92f830cadaa78d29ddcc8e23a22c5d6785f7758afdf50eba4e88238966 AS chef

RUN apk add --no-cache musl-dev

WORKDIR /app

FROM chef AS planner

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

ENV K8S_OPENAPI_ENABLED_VERSION=1.32

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
    cargo chef cook --release -p pipedash-web --recipe-path recipe.json --target $(cat /tmp/rust_target)

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=frontend-builder /app/crates/pipedash-web/dist ./crates/pipedash-web/dist

ENV SKIP_FRONTEND_BUILD=1
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release -p pipedash-web --target $(cat /tmp/rust_target)

RUN strip target/$(cat /tmp/rust_target)/release/pipedash-web && \
    cp target/$(cat /tmp/rust_target)/release/pipedash-web /pipedash-web && \
    mkdir -p /runtime/data /runtime/tmp && chmod 1777 /runtime/tmp

FROM scratch

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /pipedash-web /pipedash-web
COPY --from=builder --chown=65532:65532 /runtime/data /data
COPY --from=builder --chown=65532:65532 /runtime/tmp /tmp

ENV PIPEDASH_DATA_DIR=/data
ENV PIPEDASH_CONFIG_PATH=/data/config.toml

VOLUME /data
USER 65532:65532

ENTRYPOINT ["/pipedash-web"]
