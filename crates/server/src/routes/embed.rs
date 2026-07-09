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

            let html = format!(
                r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title} — Medal Clone</title>
<meta property="og:type" content="video.other">
<meta property="og:title" content="{title}">
<meta property="og:video" content="{video_url}">
<meta property="og:video:secure_url" content="{video_url}">
<meta property="og:video:type" content="video/mp4">
<meta property="og:image" content="{thumb_url}">
<meta name="twitter:card" content="player">
<meta name="twitter:player" content="{base_url}/clip/{slug}/embed">
<link rel="alternate" type="application/json+oembed" href="{base_url}/oembed?url={base_url}/clip/{slug}">
</head>
<body>
<video controls autoplay muted style="max-width:100%;max-height:100vh">
<source src="{video_url}" type="video/mp4">
</video>
</body>
</html>"#,
                title = title,
                video_url = video_url,
                thumb_url = thumb_url,
                base_url = cfg.server.base_url,
                slug = slug,
            );

            let _ = sqlx::query("UPDATE clips SET view_count = view_count + 1 WHERE slug = ?")
                .bind(&slug)
                .execute(pool.get_ref())
                .await;

            Ok(HttpResponse::Ok()
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
            let autoplay = if query.autoplay.unwrap_or(1) == 1 { "autoplay" } else { "" };
            let muted = if query.muted.unwrap_or(1) == 1 { "muted" } else { "" };
            let loop_attr = if query.r#loop.unwrap_or(0) == 1 { "loop" } else { "" };
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
                title = title,
                autoplay = autoplay,
                muted = muted,
                loop_attr = loop_attr,
                video_url = video_url,
            );

            Ok(HttpResponse::Ok()
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
    query: web::Query<OEmbedQuery>,
) -> actix_web::Result<HttpResponse> {
    if !query.url.contains("/clip/") {
        return Ok(HttpResponse::NotFound().finish());
    }

    if query.format.as_deref() == Some("json") || query.format.is_none() {
        let slug = query.url.rsplit('/').next().unwrap_or("");
        let width = query.maxwidth.unwrap_or(640).min(640);
        let height = query.maxheight.unwrap_or(360).min(360);

        let json = serde_json::json!({
            "version": "1.0",
            "type": "video",
            "provider_name": "MedalClone",
            "provider_url": "",
            "title": "",
            "html": format!(
                "<iframe src='/clip/{slug}/embed' width='{width}' height='{height}' frameborder='0' allowfullscreen></iframe>",
                slug = slug,
                width = width,
                height = height,
            ),
            "width": width,
            "height": height,
        });

        Ok(HttpResponse::Ok()
            .content_type("application/json; charset=utf-8")
            .json(json))
    } else {
        Ok(HttpResponse::NotImplemented().finish())
    }
}
