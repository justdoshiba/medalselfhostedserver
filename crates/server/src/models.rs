use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Clip {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub filename: String,
    pub thumbnail_path: Option<String>,
    pub duration_secs: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_size_bytes: i64,
    pub created_at: String,
    pub view_count: i64,
}

impl Clip {
    pub fn to_common(&self, _base_url: &str) -> medal_clone_common::Clip {
        medal_clone_common::Clip {
            id: Uuid::parse_str(&self.id).unwrap_or_default(),
            slug: self.slug.clone(),
            title: self.title.clone(),
            filename: self.filename.clone(),
            thumbnail_path: self.thumbnail_path.clone(),
            duration_secs: self.duration_secs,
            width: self.width.map(|w| w as u32),
            height: self.height.map(|h| h as u32),
            file_size_bytes: self.file_size_bytes as u64,
            created_at: DateTime::parse_from_rfc3339(&self.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            view_count: self.view_count as u64,
        }
    }
}
