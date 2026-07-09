# syntax=docker/dockerfile:1
FROM rust:slim-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p medal-clone-server && cargo build --release -p medal-clone-watcher

FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    ffmpeg \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/medal-clone-server /app/medal-clone-server
COPY --from=builder /app/target/release/medal-clone-watcher /app/medal-clone-watcher
RUN mkdir -p /app/data/db /app/data/storage

EXPOSE 8080

ENV HOST=0.0.0.0
ENV PORT=8080
ENV DATABASE_URL=sqlite:/app/data/db/medal-clone.db?mode=rwc
ENV DATA_DIR=/app/data

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

CMD ["/app/medal-clone-server"]
