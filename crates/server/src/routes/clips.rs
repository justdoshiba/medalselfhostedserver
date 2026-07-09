use actix_web::{web, HttpRequest, HttpResponse};
use sqlx::sqlite::SqlitePool;

use crate::config::Config;
use crate::models::Clip;
use crate::storage::{parse_range_header, Storage};

pub async fn get_clip(
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
            let _ = sqlx::query("UPDATE clips SET view_count = view_count + 1 WHERE slug = ?")
                .bind(&slug)
                .execute(pool.get_ref())
                .await;

            let common = c.to_common(&cfg.server.base_url);
            Ok(HttpResponse::Ok().json(common))
        }
        None => Ok(HttpResponse::NotFound().finish()),
    }
}

pub async fn list_clips(
    pool: web::Data<SqlitePool>,
    cfg: web::Data<Config>,
) -> actix_web::Result<HttpResponse> {
    let clips = sqlx::query_as::<_, Clip>(
        "SELECT * FROM clips ORDER BY created_at DESC LIMIT 100",
    )
    .fetch_all(pool.get_ref())
    .await
    .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    let common: Vec<medal_clone_common::Clip> = clips
        .into_iter()
        .map(|c| c.to_common(&cfg.server.base_url))
        .collect();

    Ok(HttpResponse::Ok().json(common))
}

pub async fn delete_clip(
    pool: web::Data<SqlitePool>,
    storage: web::Data<Storage>,
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
            let _ = storage.delete_object(&c.filename).await;
            if let Some(ref thumb) = c.thumbnail_path {
                let _ = storage.delete_object(thumb).await;
            }

            sqlx::query("DELETE FROM clips WHERE slug = ?")
                .bind(&slug)
                .execute(pool.get_ref())
                .await
                .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

            Ok(HttpResponse::Ok().json(serde_json::json!({"deleted": true})))
        }
        None => Ok(HttpResponse::NotFound().finish()),
    }
}

pub async fn update_clip(
    pool: web::Data<SqlitePool>,
    cfg: web::Data<Config>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
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

    let slug = path.into_inner();
    let new_title = body.get("title").and_then(|v| v.as_str()).unwrap_or("");

    if !new_title.is_empty() {
        sqlx::query("UPDATE clips SET title = ? WHERE slug = ?")
            .bind(new_title)
            .bind(&slug)
            .execute(pool.get_ref())
            .await
            .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({"updated": true})))
}

pub async fn serve_video(
    pool: web::Data<SqlitePool>,
    storage: web::Data<Storage>,
    path: web::Path<String>,
    req: HttpRequest,
) -> actix_web::Result<HttpResponse> {
    let slug = path.into_inner();

    let clip = sqlx::query_as::<_, Clip>("SELECT * FROM clips WHERE slug = ?")
        .bind(&slug)
        .fetch_optional(pool.get_ref())
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    match clip {
        Some(c) => {
            let content_type = if c.filename.ends_with(".mov") {
                "video/quicktime"
            } else if c.filename.ends_with(".webm") {
                "video/webm"
            } else {
                "video/mp4"
            };

            let range_header = req
                .headers()
                .get("Range")
                .and_then(|v| v.to_str().ok());

            if let Some(range_str) = range_header {
                if let Some((start, end)) = parse_range_header(range_str) {
                    match storage
                        .get_object_range(&c.filename, start, end)
                        .await
                    {
                        Ok(resp) => {
                            return Ok(HttpResponse::PartialContent()
                                .insert_header(("Content-Type", content_type))
                                .insert_header(("Accept-Ranges", "bytes"))
                                .insert_header((
                                    "Content-Range",
                                    resp.content_range.as_str(),
                                ))
                                .insert_header((
                                    "Content-Length",
                                    resp.content_length.to_string(),
                                ))
                                .insert_header((
                                    "Cache-Control",
                                    "public, max-age=31536000, immutable",
                                ))
                                .body(resp.data));
                        }
                        Err(e) => {
                            return Ok(HttpResponse::RangeNotSatisfiable()
                                .insert_header(("Content-Range", format!("bytes */{}", 0)))
                                .body(format!("range error: {e}")));
                        }
                    }
                }
            }

            match storage.get_object(&c.filename).await {
                Ok(data) => Ok(HttpResponse::Ok()
                    .insert_header(("Content-Type", content_type))
                    .insert_header(("Accept-Ranges", "bytes"))
                    .insert_header(("Content-Length", data.len().to_string()))
                    .insert_header((
                        "Cache-Control",
                        "public, max-age=31536000, immutable",
                    ))
                    .body(data)),
                Err(e) => Ok(HttpResponse::InternalServerError()
                    .body(format!("s3 error: {e}"))),
            }
        }
        None => Ok(HttpResponse::NotFound().finish()),
    }
}

pub async fn serve_thumbnail(
    pool: web::Data<SqlitePool>,
    storage: web::Data<Storage>,
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
            let thumb_key = match c.thumbnail_path {
                Some(ref k) => k.clone(),
                None => return Ok(HttpResponse::NotFound().finish()),
            };

            match storage.get_object(&thumb_key).await {
                Ok(data) => Ok(HttpResponse::Ok()
                    .insert_header(("Content-Type", "image/jpeg"))
                    .insert_header(("Content-Length", data.len().to_string()))
                    .insert_header(("Cache-Control", "public, max-age=86400"))
                    .body(data)),
                Err(_) => Ok(HttpResponse::NotFound().finish()),
            }
        }
        None => Ok(HttpResponse::NotFound().finish()),
    }
}