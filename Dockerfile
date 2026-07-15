FROM node:22-bookworm-slim AS web-builder
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:1.97-bookworm AS rust-builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY apps/ apps/
RUN cargo build --locked --release -p narrastate-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust-builder /src/target/release/narrastate-server /usr/local/bin/narrastate-server
COPY --from=web-builder /src/web/dist /app/web/dist
COPY cases/ /app/cases/
RUN mkdir -p /app/data && chown -R nobody:nogroup /app/data
USER nobody
ENV NARRASTATE_HOST=0.0.0.0 \
    NARRASTATE_PORT=3000 \
    NARRASTATE_WEB_DIR=/app/web/dist \
    DATABASE_URL=/app/data/narrastate.db \
    RUST_LOG=info
EXPOSE 3000
VOLUME ["/app/data"]
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD curl --fail --silent http://127.0.0.1:3000/api/v1/health >/dev/null || exit 1
CMD ["narrastate-server", "serve", "--cases", "/app/cases", "--web", "/app/web/dist"]
