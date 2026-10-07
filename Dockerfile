# syntax=docker/dockerfile:1

FROM node:24-bookworm-slim AS frontend-base
ENV NEXT_TELEMETRY_DISABLED=1

FROM frontend-base AS admin-frontend
WORKDIR /src/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY frontend/ ./
# The administrator contract tests read the server's checked-in fixtures.
COPY crates/codex2api-admin/tests/contracts.json /src/crates/codex2api-admin/tests/contracts.json
RUN npm run lint && npm run format:check && npm run typecheck && npm test && npm run build \
    && rm -rf node_modules .next

FROM frontend-base AS user-frontend
WORKDIR /src/frontend-user
COPY frontend-user/package.json frontend-user/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY frontend-user/ ./
RUN npm run lint && npm run format:check && npm run typecheck && npm test && npm run build \
    && rm -rf node_modules .next

FROM rust:1.93.0-slim-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential ca-certificates cmake git libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
# Keep the exact sources alongside each export for codex2api-web's SHA-256 checks.
COPY --from=admin-frontend /src/frontend/ frontend/
COPY --from=user-frontend /src/frontend-user/ frontend-user/
ARG CODEX2API_BUILD_TAG=unknown
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --locked --release --package codex2api

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libgcc-s1 libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 codex2api \
    && useradd --uid 10001 --gid codex2api --no-create-home --home-dir /app --shell /usr/sbin/nologin codex2api \
    && install -d -o codex2api -g codex2api /app/data
WORKDIR /app
COPY --from=builder /src/target/release/codex2api /usr/local/bin/codex2api
ENV CODEX2API_API_BIND=0.0.0.0:8080 \
    CODEX2API_ADMIN_BIND=0.0.0.0:8081 \
    CODEX2API_USER_BIND=0.0.0.0:8082 \
    CODEX2API_DB=/app/data/codex2api.sqlite
USER 10001:10001
EXPOSE 8080 8081 8082
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD curl --fail --silent --show-error --max-time 3 http://127.0.0.1:8080/healthz > /dev/null \
        && curl --fail --silent --show-error --max-time 3 http://127.0.0.1:8081/admin/ > /dev/null \
        && curl --fail --silent --show-error --max-time 3 http://127.0.0.1:8082/user/ > /dev/null
ENTRYPOINT ["/usr/local/bin/codex2api"]
