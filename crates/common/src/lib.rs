use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub filename: String,
    pub thumbnail_path: Option<String>,
    pub duration_secs: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub file_size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub view_count: u64,
}

#[derive(Debug, Deserialize)]
pub struct UploadToken {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub id: Uuid,
    pub url: String,
}
