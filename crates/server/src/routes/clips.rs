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
                                .insert_header(("Content-Type", "video/mp4"))
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
                    .insert_header(("Content-Type", "video/mp4"))
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
