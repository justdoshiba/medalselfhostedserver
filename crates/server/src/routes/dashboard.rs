use crate::config::Config;
use actix_web::{web, HttpRequest, HttpResponse};
use sqlx::sqlite::SqlitePool;

pub async fn dashboard_page() -> HttpResponse {
    let html = include_str!("../dashboard.html");
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

pub async fn dashboard_data(
    pool: web::Data<SqlitePool>,
    cfg: web::Data<Config>,
    req: HttpRequest,
) -> actix_web::Result<HttpResponse> {
    let auth_token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    match auth_token {
        Some(t) if t == cfg.server.upload_token => {}
        _ => return Ok(HttpResponse::Unauthorized().finish()),
    }

    let clips = sqlx::query_as::<_, crate::models::Clip>(
        "SELECT * FROM clips ORDER BY created_at DESC LIMIT 100",
    )
    .fetch_all(pool.get_ref())
    .await
    .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    let items: Vec<serde_json::Value> = clips
        .into_iter()
        .map(|c| {
            let title = if c.title.is_empty() { &c.filename } else { &c.title };
            serde_json::json!({
                "slug": c.slug,
                "title": title,
                "filename": c.filename,
                "duration": c.duration_secs,
                "width": c.width,
                "height": c.height,
                "file_size_bytes": c.file_size_bytes,
                "created_at": c.created_at,
                "view_count": c.view_count,
                "thumb_url": format!("{}/api/clips/{}/thumb", cfg.server.base_url, c.slug),
                "video_url": format!("{}/api/clips/{}/video", cfg.server.base_url, c.slug),
                "clip_url": format!("{}/clip/{}", cfg.server.base_url, c.slug),
            })
        })
        .collect();

    let resp = serde_json::json!({
        "clips": items,
        "base_url": cfg.server.base_url,
    });

    Ok(HttpResponse::Ok().json(resp))
}