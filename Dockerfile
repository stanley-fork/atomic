# syntax=docker/dockerfile:1
# =============================================================================
# Stage 1a: Cargo Chef planner (dependency caching)
# =============================================================================
FROM rust:1.94-bookworm AS planner
RUN cargo install cargo-chef
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Stage 1b: Cargo Chef cook + build atomic-server
# =============================================================================
FROM rust:1.94-bookworm AS rust-builder

# Install mold linker + cargo-chef
RUN apt-get update && apt-get install -y --no-install-recommends mold && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef
WORKDIR /app

# Copy linker config
COPY .cargo/ .cargo/

# Cook dependencies (cached until Cargo.toml/lock changes)
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo chef cook --profile server --recipe-path recipe.json -p atomic-server

# Copy real workspace source
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Workspace stubs for crates we don't build but Cargo needs for resolution
COPY src-tauri/Cargo.toml src-tauri/Cargo.toml
RUN mkdir -p src-tauri/src && \
    echo "fn main() {}" > src-tauri/src/main.rs && \
    echo "pub fn lib() {}" > src-tauri/src/lib.rs && \
    echo "fn main() { tauri_build::build(); }" > src-tauri/build.rs

# Build atomic-server with the faster server profile
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --profile server -p atomic-server && \
    cp /app/target/server/atomic-server /usr/local/bin/atomic-server

# =============================================================================
# Stage 2: Frontend builder
# =============================================================================
FROM node:24-bookworm-slim AS frontend-builder
WORKDIR /app

# Install dependencies (cached layer)
# --ignore-scripts skips better-sqlite3's native compile (it's a dev-only dep
# used by local db scripts, not needed for `vite build`). Without this we'd
# need python3/make/g++ in this stage.
COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts

# Copy frontend source
COPY index.html tsconfig.json tsconfig.node.json vite.config.ts ./
COPY src/ src/
COPY public/ public/

# Build web target
RUN VITE_BUILD_TARGET=web npm run build:web

# =============================================================================
# Stage 3: Server runtime
# =============================================================================
FROM debian:bookworm-slim AS server

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --shell /bin/false atomic && \
    mkdir -p /data && chown atomic:atomic /data

COPY --from=rust-builder /usr/local/bin/atomic-server /usr/local/bin/atomic-server

USER atomic
VOLUME /data
EXPOSE 8080

ENTRYPOINT ["atomic-server", "--db-path", "/data/atomic.db"]
CMD ["serve", "--bind", "0.0.0.0", "--port", "8080"]

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# =============================================================================
# Stage 4: Web (nginx) runtime
# =============================================================================
FROM nginx:1.28-bookworm AS web

RUN rm /etc/nginx/conf.d/default.conf
COPY docker/nginx.conf /etc/nginx/conf.d/atomic.conf
COPY --from=frontend-builder /app/dist-web/ /usr/share/nginx/html/

EXPOSE 80

# =============================================================================
# Stage 5: All-in-one (server + nginx + frontend) for single-container deploys
# =============================================================================
FROM debian:bookworm-slim AS all-in-one

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates curl nginx supervisor && \
    rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --shell /bin/false atomic && \
    mkdir -p /data && chown atomic:atomic /data

COPY --from=rust-builder /usr/local/bin/atomic-server /usr/local/bin/atomic-server
COPY --from=frontend-builder /app/dist-web/ /usr/share/nginx/html/

# Nginx config (proxies to atomic-server on localhost)
RUN rm -f /etc/nginx/sites-enabled/default
COPY docker/nginx-fly.conf /etc/nginx/conf.d/atomic.conf

# Supervisord config
COPY docker/supervisord.conf /etc/supervisor/conf.d/supervisord.conf

VOLUME /data
EXPOSE 8081

HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

CMD ["sh", "-c", "chown -R atomic:atomic /data && exec supervisord -c /etc/supervisor/conf.d/supervisord.conf"]

# =============================================================================
# Stage 6: Server runtime (Postgres)
# =============================================================================
# Same atomic-server binary as the SQLite `server` stage above (the postgres
# feature is enabled by default in atomic-server's Cargo.toml). The stage only
# differs in runtime configuration:
#   - ATOMIC_STORAGE=postgres puts the server in Postgres mode.
#   - The entrypoint drops --db-path; the operator supplies ATOMIC_DATABASE_URL
#     (e.g. via Fly secrets, docker run -e, or compose).
#   - No /data volume — Postgres holds all state.
FROM debian:bookworm-slim AS server-postgres

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --shell /bin/false atomic

COPY --from=rust-builder /usr/local/bin/atomic-server /usr/local/bin/atomic-server

ENV ATOMIC_STORAGE=postgres

USER atomic
EXPOSE 8080

ENTRYPOINT ["atomic-server"]
CMD ["serve", "--bind", "0.0.0.0", "--port", "8080"]

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# =============================================================================
# Stage 7: All-in-one (Postgres) — server + nginx + frontend, no /data
# =============================================================================
# Mirrors `all-in-one` but with the Postgres storage env set. Reuses the same
# supervisord/nginx configs; the --data-dir flag inside supervisord.conf is
# inert in Postgres mode (data_dir is unused once ATOMIC_STORAGE=postgres).
FROM debian:bookworm-slim AS all-in-one-postgres

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates curl nginx supervisor && \
    rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --shell /bin/false atomic && \
    mkdir -p /data && chown atomic:atomic /data

COPY --from=rust-builder /usr/local/bin/atomic-server /usr/local/bin/atomic-server
COPY --from=frontend-builder /app/dist-web/ /usr/share/nginx/html/

RUN rm -f /etc/nginx/sites-enabled/default
COPY docker/nginx-fly.conf /etc/nginx/conf.d/atomic.conf
COPY docker/supervisord.conf /etc/supervisor/conf.d/supervisord.conf

ENV ATOMIC_STORAGE=postgres

EXPOSE 8081

HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

CMD ["sh", "-c", "exec supervisord -c /etc/supervisor/conf.d/supervisord.conf"]
