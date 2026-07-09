use actix_web::{web, HttpResponse};
use sqlx::sqlite::SqlitePool;

use crate::config::Config;
use crate::models::Clip;

pub async fn clip_page(
    pool: web::Data<SqlitePool>,
    cfg: web::Data<Config>,
    path: web::Path<String>,
) -> actix_web::Result<HttpResponse> {
    let slug = path.into_inner();

    let clip = sqlx::query_as::<_, Clip>("SELECT * FROM clips WHERE slug = ?")
        .bind(&slug)
        .fetch_optional(pool.get_ref())
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    match clip {
        Some(c) => {
            let video_url = format!("{}/api/clips/{}/video", cfg.server.base_url, slug);
            let thumb_url = format!("{}/api/clips/{}/thumb", cfg.server.base_url, slug);
            let title = if c.title.is_empty() { &c.filename } else { &c.title };

            let og_width = c.width.unwrap_or(1920);
            let og_height = c.height.unwrap_or(1080);

            let description = match c.duration_secs {
                Some(s) => format!("{:.0}s clip", s),
                None => "Clip".into(),
            };

            let html = format!(
                r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title}</title>
<meta property="og:type" content="video.other">
<meta property="og:title" content="{title}">
<meta property="og:description" content="{description}">
<meta property="og:video" content="{video_url}">
<meta property="og:video:secure_url" content="{video_url}">
<meta property="og:video:type" content="video/mp4">
<meta property="og:video:width" content="{og_width}">
<meta property="og:video:height" content="{og_height}">
<meta property="og:image" content="{thumb_url}">
<meta name="twitter:card" content="player">
<meta name="twitter:player" content="{base_url}/clip/{slug}/embed">
<meta name="twitter:player:width" content="{og_width}">
<meta name="twitter:player:height" content="{og_height}">
<link rel="alternate" type="application/json+oembed" href="{base_url}/oembed?url={base_url}/clip/{slug}">
</head>
<body>
<video controls autoplay muted style="max-width:100%;max-height:100vh">
<source src="{video_url}" type="video/mp4">
</video>
</body>
</html>"#,
                title = title,
                description = description,
                video_url = video_url,
                og_width = og_width,
                og_height = og_height,
                thumb_url = thumb_url,
                base_url = cfg.server.base_url,
                slug = slug,
            );

            let _ = sqlx::query("UPDATE clips SET view_count = view_count + 1 WHERE slug = ?")
                .bind(&slug)
                .execute(pool.get_ref())
                .await;

            Ok(HttpResponse::Ok()
                .insert_header(("X-Content-Type-Options", "nosniff"))
                .content_type("text/html; charset=utf-8")
                .body(html))
        }
        None => Ok(HttpResponse::NotFound().finish()),
    }
}

pub async fn embed_iframe(
    pool: web::Data<SqlitePool>,
    cfg: web::Data<Config>,
    path: web::Path<String>,
    query: web::Query<EmbedParams>,
) -> actix_web::Result<HttpResponse> {
    let slug = path.into_inner();

    let clip = sqlx::query_as::<_, Clip>("SELECT * FROM clips WHERE slug = ?")
        .bind(&slug)
        .fetch_optional(pool.get_ref())
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    match clip {
        Some(c) => {
            let video_url = format!("{}/api/clips/{}/video", cfg.server.base_url, slug);
            let autoplay = if query.autoplay.unwrap_or(1) == 1 {
                "autoplay"
            } else {
                ""
            };
            let muted = if query.muted.unwrap_or(1) == 1 {
                "muted"
            } else {
                ""
            };
            let loop_attr = if query.r#loop.unwrap_or(0) == 1 {
                "loop"
            } else {
                ""
            };
            let title = if c.title.is_empty() { &c.filename } else { &c.title };

            let html = format!(
                r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title}</title>
<style>
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{ background:#000; display:flex; align-items:center; justify-content:center; height:100vh; }}
video {{ max-width:100%; max-height:100vh; }}
</style>
</head>
<body>
<video controls {autoplay} {muted} {loop_attr} style="max-width:100%;max-height:100vh">
<source src="{video_url}" type="video/mp4">
</video>
</body>
</html>"#,
            );

            Ok(HttpResponse::Ok()
                .insert_header(("X-Content-Type-Options", "nosniff"))
                .content_type("text/html; charset=utf-8")
                .body(html))
        }
        None => Ok(HttpResponse::NotFound().finish()),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct EmbedParams {
    autoplay: Option<u8>,
    muted: Option<u8>,
    r#loop: Option<u8>,
}

#[derive(Debug, serde::Deserialize)]
pub struct OEmbedQuery {
    url: String,
    maxwidth: Option<u32>,
    maxheight: Option<u32>,
    format: Option<String>,
}

pub async fn oembed(
    pool: web::Data<SqlitePool>,
    cfg: web::Data<Config>,
    query: web::Query<OEmbedQuery>,
) -> actix_web::Result<HttpResponse> {
    if !query.url.contains("/clip/") {
        return Ok(HttpResponse::NotFound().finish());
    }

    if query.format.is_some() && query.format.as_deref() != Some("json") {
        return Ok(HttpResponse::NotImplemented().finish());
    }

    let slug = query
        .url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");

    if slug.is_empty() {
        return Ok(HttpResponse::NotFound().finish());
    }

    let clip = sqlx::query_as::<_, Clip>("SELECT * FROM clips WHERE slug = ?")
        .bind(&slug)
        .fetch_optional(pool.get_ref())
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    let (title, thumb_url) = match clip {
        Some(c) => {
            let t = if c.title.is_empty() { c.filename } else { c.title };
            let thumb = format!("{}/api/clips/{}/thumb", cfg.server.base_url, slug);
            (t, thumb)
        }
        None => (String::new(), String::new()),
    };

    let width = query.maxwidth.unwrap_or(640).min(640);
    let height = query.maxheight.unwrap_or(360).min(360);

    let iframe_url = format!("{}/clip/{}/embed", cfg.server.base_url, slug);

    let json = serde_json::json!({
        "version": "1.0",
        "type": "video",
        "provider_name": "MedalClone",
        "provider_url": cfg.server.base_url,
        "title": title,
        "html": format!(
            r#"<iframe src="{iframe_url}" width="{width}" height="{height}" frameborder="0" allowfullscreen></iframe>"#,
        ),
        "width": width,
        "height": height,
        "thumbnail_url": thumb_url,
        "thumbnail_width": width,
        "thumbnail_height": height,
    });

    Ok(HttpResponse::Ok()
        .content_type("application/json; charset=utf-8")
        .json(json))
}
