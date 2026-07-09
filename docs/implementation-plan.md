# Implementation Plan

## Architecture

```
                            ┌──────────────┐
                            │   Caddy       │  ← auto HTTPS, reverse proxy
                            │  (in Coolify  │
                            │   or standalone)│
                            └──────┬───────┘
                                   │
┌────────────────┐         ┌───────┴────────┐         ┌──────────────┐
│ Medal Desktop   │         │  server binary  │         │   Garage     │
│ App → local     │  POST   │  (Actix-web)    │  S3 API  │  (object     │
│ .mp4 files     ├────────▶│                 ├────────▶│   storage)   │
│                │  multipart│  sqlx + SQLite  │         │              │
└───────┬────────┘         │    (metadata)    │         └──────────────┘
        │                  └────────┬────────┘
        │                           │
        │  ┌────────────┐          │
        └──┤  watcher   │          │
           │  binary    │──────────┘
           │  (notify + │  HTTP POST
           │   reqwest) │  multipart
           └────────────┘
```

**Coolify services to spin up:**
- **Server app** — builds from Dockerfile, 2 env vars for Garage credentials
- **Garage** — one-click service (S3-compatible object storage)
- **Caddy** — can be Coolify's built-in proxy + your custom Caddyfile, or a separate Caddy service for full control

**Persistent volumes needed:**
- `/app/data/db` — SQLite database file (small, metadata only)

Everything else (videos, thumbs) lives in Garage.

---

## Step 1 — Add S3 dependency + config

### New crate: `rust-s3` (lightweight, no AWS SDK bloat)

Add to `crates/server/Cargo.toml`:

```toml
rust-s3 = "0.37"
```

### New config fields (`config.rs`)

```rust
pub struct S3Config {
    pub endpoint: String,     // Garage endpoint URL
    pub region: String,       // Garage region (e.g. "garage")
    pub bucket: String,       // bucket name
    pub access_key: String,   // Garage access key
    pub secret_key: String,   // Garage secret key
}
```

Env vars: `S3_ENDPOINT`, `S3_REGION`, `S3_BUCKET`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`.

### New module: `storage.rs`

Abstract the S3 operations behind a clean interface:

```rust
pub struct Storage { bucket: Bucket }

impl Storage {
    pub async fn put_video(&self, key: &str, data: bytes::Bytes, content_type: &str)
    pub async fn put_thumbnail(&self, key: &str, data: bytes::Bytes)

    pub async fn get_video_range(&self, key: &str, range: Option<&str>)
        -> Result<(Vec<u8>, u64, u64, u64)>  // (data, content_length, start, end)

    pub async fn get_object(&self, key: &str) -> Result<Vec<u8>>
}
```

`rust-s3`'s `Bucket` has built-in `get_object_range()` and `put_object()` that map directly to Garage's S3 API.

---

## Step 2 — Upload Endpoint (`routes/upload.rs`)

### Flow
1. **Auth** — validate `Authorization: Bearer <token>` against `cfg.server.upload_token`.
2. **Multipart receive** — stream to a temp file on disk (using `actix-multipart` + `tokio::fs`).
3. **FFprobe** — after file fully received, spawn `ffprobe` to extract duration, width, height. On failure, log warning and continue with NULLs.
4. **Thumbnail** — spawn `ffmpeg -ss 00:00:01 -i <tmp> -vframes 1 -q:v 2 <tmp_thumb>`.
5. **Upload to Garage** — read temp file into memory (or stream), call `storage.put_video()`. Read thumb, call `storage.put_thumbnail()`.
6. **DB insert** — record the S3 key (`{uuid}.mp4`), thumbnail S3 key, metadata.
7. **Cleanup** — delete temp files.
8. **Response** — `{"id": "uuid", "url": "https://domain/clip/{slug}"}`.

### Downsides of current approach
- Temp file on disk is required because ffprobe/ffmpeg need a real file path.
- Acceptable: ffmpeg can't read from stdin for probing, and the file is already fully buffered by the multipart stream anyway.

---

## Step 3 — Range-Request Video Serving (`routes/clips.rs`)

### `serve_video` — stream from Garage with range forwarding

Replace the current `todo!()`:

1. Look up clip by slug in DB → get S3 filename key.
2. Parse `Range` header from `HttpRequest`.
3. Call `storage.get_video_range(key, range_header_value)` which calls `Bucket::get_object_range()`.
4. Build response:
   - If range was requested → `206 Partial Content` with `Content-Range` header.
   - If no range → `200 OK` with full file.
   - Set `Accept-Ranges: bytes`, `Content-Type: video/mp4`, `Content-Length`.
5. Stream the body bytes.

Garage natively supports S3 range requests (`Range` header on `GetObject`), so this is a straightforward pass-through.

### `serve_thumbnail` — fetch from Garage

```rust
let data = storage.get_object(&thumb_key).await?;
HttpResponse::Ok()
    .content_type("image/jpeg")
    .insert_header(("Cache-Control", "public, max-age=86400"))
    .body(data)
```

---

## Step 4 — Embed Page (`routes/embed.rs`)

Already structurally complete. Polish needed:

- `og:video:width` / `og:video:height` — read from DB instead of hardcoded 1920×1080.
- `og:description` — include `"${duration}s clip"`.
- oEmbed — read clip title from DB, include `thumbnail_url` with absolute URL, make iframe `src` absolute.
- Set `X-Content-Type-Options: nosniff` on HTML responses.

---

## Step 5 — Watcher Binary (`crates/watcher/src/main.rs`)

No changes from the previously planned approach — it still POSTs multipart to the server. The server handles S3 storage; the watcher doesn't know about Garage.

### File stability
On `Create`/`Modify` event, poll file size every 500ms for up to 10s. Once stable, upload.

### Upload
`reqwest::Client::post(url).multipart(...)`. Retry 3x on failure.

---

## Step 6 — Caddyfile

Caddy sits in front of Actix. Minimal config:

```caddy
clips.example.com {
    reverse_proxy medal-clone-server:8080
    request_body max_size 5000MB

    header {
        X-Content-Type-Options "nosniff"
        X-Frame-Options "DENY"
    }
}
```

In Coolify, you can either:
- **Option A:** Use Coolify's built-in Caddy proxy (no separate Caddy service) — set the domain in Coolify's UI.
- **Option B:** Run Caddy as a separate service in Coolify with the Caddyfile mounted as a config volume.

Option A is simpler and recommended. Only need Option B if you want Caddy-specific directives that Coolify's UI doesn't expose.

---

## Step 7 — Dockerfile

No change needed — still works. The `ffmpeg` package is already in the runtime stage. No storage volume needed (just the `db` volume).

```dockerfile
FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p medal-clone-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl ffmpeg && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/medal-clone-server /app/
RUN mkdir -p /app/data/db

EXPOSE 8080
HEALTHCHECK CMD curl -f http://localhost:8080/health || exit 1
CMD ["/app/medal-clone-server"]
```

---

## New Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `rust-s3` | `0.37` | S3 API client for Garage |
| `bytes` | `1.12` | Buffer for S3 uploads |

---

## Route Summary (unchanged)

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| POST | `/api/upload` | `upload::upload_clip` | Receive video + metadata |
| GET | `/api/clips/{slug}` | `clips::get_clip` | Clip metadata JSON |
| GET | `/api/clips/{slug}/video` | `clips::serve_video` | Range-request video from Garage |
| GET | `/api/clips/{slug}/thumb` | `clips::serve_thumbnail` | Thumbnail from Garage |
| GET | `/clip/{slug}` | `embed::clip_page` | HTML page + OG tags |
| GET | `/clip/{slug}/embed` | `embed::embed_iframe` | Embeddable iframe player |
| GET | `/oembed` | `embed::oembed` | oEmbed JSON endpoint |
| GET | `/health` | inline | Coolify health check |
