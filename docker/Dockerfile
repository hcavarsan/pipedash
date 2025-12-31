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

COPY --from=planner /app/recipe.json recipe.json

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release -p pipedash-web --recipe-path recipe.json --target x86_64-unknown-linux-musl

COPY --from=frontend-builder /app/dist ./dist

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

ENV SKIP_FRONTEND_BUILD=1
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release -p pipedash-web --target x86_64-unknown-linux-musl

RUN strip target/x86_64-unknown-linux-musl/release/pipedash-web

FROM scratch

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/pipedash-web /pipedash-web

ENTRYPOINT ["/pipedash-web"]
