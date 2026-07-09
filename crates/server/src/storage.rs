use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
}

pub struct RangeResponse {
    pub data: Vec<u8>,
    pub content_length: u64,
    pub content_range: String,
}

pub enum Storage {
    S3(S3Storage),
    Local(LocalStorage),
}

pub struct S3Storage {
    bucket: Arc<Mutex<Bucket>>,
}

pub struct LocalStorage {
    data_dir: PathBuf,
}

impl Storage {
    pub fn from_config(cfg: &S3Config) -> Self {
        let region = Region::Custom {
            region: cfg.region.clone(),
            endpoint: cfg.endpoint.clone(),
        };
        let credentials = Credentials::new(
            Some(&cfg.access_key),
            Some(&cfg.secret_key),
            None,
            None,
            None,
        )
        .expect("invalid S3 credentials");
        let bucket = Bucket::new(&cfg.bucket, region, credentials)
            .expect("invalid S3 bucket config")
            .with_path_style();
        Storage::S3(S3Storage {
            bucket: Arc::new(Mutex::new(*bucket)),
        })
    }

    pub fn local(data_dir: PathBuf) -> Self {
        Storage::Local(LocalStorage { data_dir })
    }

    pub async fn put_video(&self, key: &str, data: &[u8]) -> anyhow::Result<()> {
        match self {
            Storage::S3(s) => {
                let bucket = s.bucket.lock().await;
                bucket
                    .put_object_with_content_type(key, data, "video/mp4")
                    .await?;
                Ok(())
            }
            Storage::Local(l) => {
                let path = l.data_dir.join("videos").join(key);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(&path, data).await?;
                Ok(())
            }
        }
    }

    pub async fn put_thumbnail(&self, key: &str, data: &[u8]) -> anyhow::Result<()> {
        match self {
            Storage::S3(s) => {
                let bucket = s.bucket.lock().await;
                bucket
                    .put_object_with_content_type(key, data, "image/jpeg")
                    .await?;
                Ok(())
            }
            Storage::Local(l) => {
                let path = l.data_dir.join("thumbs").join(key);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(&path, data).await?;
                Ok(())
            }
        }
    }

    pub async fn get_object(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        match self {
            Storage::S3(s) => {
                let bucket = s.bucket.lock().await;
                let resp = bucket.get_object(key).await?;
                Ok(resp.bytes().to_vec())
            }
            Storage::Local(l) => {
                let path = l.data_dir.join("videos").join(key);
                if !path.exists() {
                    let thumb_path = l.data_dir.join("thumbs").join(key);
                    return Ok(tokio::fs::read(&thumb_path).await?);
                }
                Ok(tokio::fs::read(&path).await?)
            }
        }
    }

    pub async fn get_object_range(
        &self,
        key: &str,
        start: u64,
        end: Option<u64>,
    ) -> anyhow::Result<RangeResponse> {
        match self {
            Storage::S3(s) => {
                let bucket = s.bucket.lock().await;
                let resp = bucket.get_object_range(key, start, end).await?;

                let total_len = resp
                    .headers()
                    .get("content-range")
                    .map(|v| v.as_str())
                    .and_then(|v| v.split('/').nth(1).and_then(|s| s.parse::<u64>().ok()))
                    .unwrap_or(0);

                let actual_end = resp
                    .headers()
                    .get("content-range")
                    .map(|v| v.as_str())
                    .and_then(|v| {
                        v.split('/')
                            .next()
                            .and_then(|s| s.split('-').nth(1))
                            .and_then(|s| s.parse::<u64>().ok())
                    })
                    .unwrap_or(start + resp.bytes().len() as u64 - 1);

                let content_range = format!("bytes {}-{}/{}", start, actual_end, total_len);

                Ok(RangeResponse {
                    data: resp.bytes().to_vec(),
                    content_length: resp.bytes().len() as u64,
                    content_range,
                })
            }
            Storage::Local(l) => {
                let path = l.data_dir.join("videos").join(key);
                let data = tokio::fs::read(&path).await?;
                let total_len = data.len() as u64;
                let end = end.unwrap_or(total_len - 1).min(total_len - 1);
                let slice = &data[start as usize..=end as usize];
                Ok(RangeResponse {
                    data: slice.to_vec(),
                    content_length: slice.len() as u64,
                    content_range: format!("bytes {}-{}/{}", start, end, total_len),
                })
            }
        }
    }

    pub async fn delete_object(&self, key: &str) -> anyhow::Result<()> {
        match self {
            Storage::S3(s) => {
                let bucket = s.bucket.lock().await;
                bucket.delete_object(key).await?;
                Ok(())
            }
            Storage::Local(l) => {
                let path = l.data_dir.join("videos").join(key);
                let _ = tokio::fs::remove_file(&path).await;
                let thumb_path = l.data_dir.join("thumbs").join(
                    key.rsplit('.').next().map_or(key, |e| {
                        let base = &key[..key.len() - e.len() - 1];
                        base
                    })
                );
                let _ = tokio::fs::remove_file(&thumb_path).await;
                Ok(())
            }
        }
    }
}

pub fn parse_range_header(header: &str) -> Option<(u64, Option<u64>)> {
    let header = header.trim();
    if !header.starts_with("bytes=") {
        return None;
    }
    let range = &header[6..];
    let (start_str, end_str) = range.split_once('-')?;

    let start = start_str.parse::<u64>().ok()?;
    let end = if end_str.is_empty() {
        None
    } else {
        Some(end_str.parse::<u64>().ok()?)
    };

    Some((start, end))
}