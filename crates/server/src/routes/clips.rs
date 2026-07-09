use actix_web::{web, HttpRequest, HttpResponse};
use sqlx::sqlite::SqlitePool;
use std::path::PathBuf;

use crate::config::Config;
use crate::models::Clip;

pub async fn get_clip(
    pool: web::Data<SqlitePool>,
    cfg: web::Data<Config>,
    path: web::Path<String>,
) -> actix_web::Result<HttpResponse> {
    let slug = path.into_inner();

    let clip = sqlx::query_as::<_, Clip>(
        "SELECT * FROM clips WHERE slug = ?",
    )
    .bind(&slug)
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    match clip {
        Some(c) => {
            // increment view count
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
    cfg: web::Data<Config>,
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
            let video_path =
                PathBuf::from(&cfg.server.data_dir).join("storage").join(&c.filename);
            // TODO: serve file with Range header support via actix_files::NamedFile
            let _ = video_path;
            let _ = req;
            todo!("range-request video serving")
        }
        None => Ok(HttpResponse::NotFound().finish()),
    }
}

pub async fn serve_thumbnail(
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
            let thumb_path = c.thumbnail_path.map(|p| {
                PathBuf::from(&cfg.server.data_dir).join("storage").join(p)
            });
            match thumb_path {
                Some(p) if p.exists() => {
                    // TODO: serve file
                    let _ = p;
                    todo!("thumbnail serving")
                }
                _ => Ok(HttpResponse::NotFound().finish()),
            }
        }
        None => Ok(HttpResponse::NotFound().finish()),
    }
}
