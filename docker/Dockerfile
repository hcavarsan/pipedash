FROM oven/bun:latest AS frontend-builder

WORKDIR /app

COPY package.json bun.lock ./
RUN --mount=type=cache,target=/root/.bun/install/cache \
    bun install --frozen-lockfile

COPY . .
ENV VITE_API_URL=/api/v1
RUN bun run build

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
    cargo chef cook --release -p pipedash-web --recipe-path recipe.json --target $(cat /tmp/rust_target)

COPY --from=frontend-builder /app/dist ./dist

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

ENV SKIP_FRONTEND_BUILD=1
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release -p pipedash-web --target $(cat /tmp/rust_target)

RUN strip target/$(cat /tmp/rust_target)/release/pipedash-web && \
    cp target/$(cat /tmp/rust_target)/release/pipedash-web /pipedash-web

FROM scratch

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /pipedash-web /pipedash-web

ENTRYPOINT ["/pipedash-web"]
