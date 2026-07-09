use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse};
use chrono::Utc;
use futures_util::StreamExt;
use nanoid::nanoid;
use sqlx::sqlite::SqlitePool;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use std::path::PathBuf;
use crate::config::Config;
use crate::storage::Storage;

fn tmp_dir(cfg: &Config) -> PathBuf {
    PathBuf::from(&cfg.server.data_dir).join("tmp")
}

pub async fn upload_clip(
    req: HttpRequest,
    pool: web::Data<SqlitePool>,
    cfg: web::Data<Config>,
    storage: web::Data<Storage>,
    mut payload: Multipart,
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

    let clip_id = Uuid::new_v4().to_string();
    let slug = nanoid!(21);

    let mut temp_path = None;
    let mut title = String::new();
    let mut file_ext = String::from(".mp4");

    while let Some(item) = payload.next().await {
        let mut field = item.map_err(|e| {
            actix_web::error::ErrorBadRequest(format!("multipart error: {e}"))
        })?;

        let field_name = field.name().map(|s| s.to_string()).unwrap_or_default();
        let content_type = field
            .content_type()
            .map(|m| m.to_string())
            .unwrap_or_default();

        if field_name == "file" {
            if content_type.starts_with("video/") {
                file_ext = mime_guess::get_mime_extensions_str(&content_type)
                    .and_then(|exts| exts.first().map(|e| format!(".{e}")))
                    .unwrap_or_else(|| {
                        if content_type.contains("quicktime") {
                            ".mov".into()
                        } else {
                            ".mp4".into()
                        }
                    });

                let tpath = tmp_dir(&cfg).join(format!("{clip_id}{file_ext}"));
                let mut file = tokio::fs::File::create(&tpath).await.map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!(
                        "failed to create temp file: {e}"
                    ))
                })?;

                while let Some(chunk) = field.next().await {
                    let data = chunk.map_err(|e| {
                        actix_web::error::ErrorBadRequest(format!("read error: {e}"))
                    })?;
                    file.write_all(&data).await.map_err(|e| {
                        actix_web::error::ErrorInternalServerError(format!(
                            "write error: {e}"
                        ))
                    })?;
                }
                file.flush().await.ok();
                temp_path = Some(tpath);
            }
        } else if field_name == "title" {
            let mut buf = Vec::new();
            while let Some(chunk) = field.next().await {
                let data = chunk.map_err(|e| {
                    actix_web::error::ErrorBadRequest(format!("read error: {e}"))
                })?;
                buf.extend_from_slice(&data);
            }
            title = String::from_utf8_lossy(&buf).trim().to_string();
        } else {
            while let Some(_) = field.next().await {}
        }
    }

    let tpath = match temp_path {
        Some(p) => p,
        None => return Ok(HttpResponse::BadRequest().json(serde_json::json!({"error": "no video file provided"}))),
    };

    if title.is_empty() {
        title = format!("Clip {}", &slug[..8]);
    }

    let video_key = format!("{clip_id}{file_ext}");
    let thumb_key = format!("{clip_id}.jpg");

    let mut duration_secs: Option<f64> = None;
    let mut width: Option<i64> = None;
    let mut height: Option<i64> = None;

    let ffprobe_result = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(&tpath)
        .output()
        .await;

    if let Ok(output) = ffprobe_result {
        if let Ok(info) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
            duration_secs = info["format"]["duration"]
                .as_str()
                .and_then(|d| d.parse::<f64>().ok());

            if let Some(streams) = info["streams"].as_array() {
                for stream in streams {
                    if stream["codec_type"] == "video" {
                        width = stream["width"].as_i64();
                        height = stream["height"].as_i64();
                        break;
                    }
                }
            }
        }
    }

    let thumb_path = tmp_dir(&cfg).join(format!("{clip_id}.jpg"));
    let ffmpeg_result = tokio::process::Command::new("ffmpeg")
        .args([
            "-ss",
            "00:00:01",
            "-i",
        ])
        .arg(&tpath)
        .args(["-vframes", "1", "-q:v", "2", "-y"])
        .arg(&thumb_path)
        .output()
        .await;

    let file_size_bytes = tokio::fs::metadata(&tpath)
        .await
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    let video_data = tokio::fs::read(&tpath).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("failed to read temp file: {e}"))
    })?;

    storage.put_video(&video_key, &video_data).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("s3 upload failed: {e}"))
    })?;

    let thumbnail_path = if ffmpeg_result.is_ok() && thumb_path.exists() {
        let thumb_data = tokio::fs::read(&thumb_path).await.ok();
        if let Some(data) = thumb_data {
            storage.put_thumbnail(&thumb_key, &data).await.ok();
            let _ = tokio::fs::remove_file(&thumb_path).await;
            Some(thumb_key)
        } else {
            None
        }
    } else {
        None
    };

    let _ = tokio::fs::remove_file(&tpath).await;

    let created_at = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO clips (id, slug, title, filename, thumbnail_path, duration_secs, width, height, file_size_bytes, created_at, view_count) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
    )
    .bind(&clip_id)
    .bind(&slug)
    .bind(&title)
    .bind(&video_key)
    .bind(&thumbnail_path)
    .bind(duration_secs)
    .bind(width)
    .bind(height)
    .bind(file_size_bytes)
    .bind(&created_at)
    .execute(pool.get_ref())
    .await
    .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db insert failed: {e}")))?;

    let clip_url = format!("{}/clip/{}", cfg.server.base_url, slug);

    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": clip_id,
        "slug": slug,
        "url": clip_url,
    })))
}
