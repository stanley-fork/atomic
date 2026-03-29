# Stage 1: Build frontend
FROM node:22-alpine AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json* ./
RUN npm install
COPY frontend/ ./
RUN npm run build

# Stage 2: Cache Rust dependencies
FROM rust:1.87-bookworm AS deps
WORKDIR /app
RUN cargo install cargo-chef --locked
COPY Cargo.toml ./
COPY src/ src/
COPY migrations/ migrations/
# Analyze dependencies and create a recipe (lockfile-like)
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Cook dependencies (cached unless Cargo.toml changes)
FROM rust:1.87-bookworm AS builder
WORKDIR /app
RUN cargo install cargo-chef --locked
COPY --from=deps /app/recipe.json recipe.json
# Build only dependencies — this layer is cached across source changes
RUN cargo chef cook --release --recipe-path recipe.json
# Now copy source and build the actual binary (only recompiles our code)
COPY Cargo.toml ./
COPY src/ src/
COPY migrations/ migrations/
RUN cargo build --release --bin atomic-cloud

# Stage 4: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/atomic-cloud /usr/local/bin/atomic-cloud
COPY --from=frontend /app/frontend/dist /app/frontend/dist

ENV FRONTEND_DIR=/app/frontend/dist
EXPOSE 8080

ENTRYPOINT ["atomic-cloud", "serve"]
