use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse};
use sqlx::sqlite::SqlitePool;

use crate::config::Config;

pub async fn upload_clip(
    _req: HttpRequest,
    _pool: web::Data<SqlitePool>,
    _cfg: web::Data<Config>,
    _payload: Multipart,
) -> actix_web::Result<HttpResponse> {
    // TODO: validate token, stream multipart to disk, probe with ffmpeg,
    // generate thumbnail, insert DB row, return clip URL
    todo!("upload handler")
}
