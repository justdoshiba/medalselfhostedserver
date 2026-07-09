use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
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

pub struct Storage {
    bucket: Arc<Mutex<Bucket>>,
}

pub struct RangeResponse {
    pub data: Vec<u8>,
    pub content_length: u64,
    pub content_range: String,
}

impl Storage {
    pub fn new(cfg: &S3Config) -> Result<Self, s3::error::S3Error> {
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
        )?;
        let bucket = Bucket::new(&cfg.bucket, region, credentials)?
            .with_path_style();
        Ok(Storage {
            bucket: Arc::new(Mutex::new(*bucket)),
        })
    }

    pub async fn put_video(&self, key: &str, data: &[u8]) -> Result<(), s3::error::S3Error> {
        let bucket = self.bucket.lock().await;
        bucket
            .put_object_with_content_type(key, data, "video/mp4")
            .await?;
        Ok(())
    }

    pub async fn put_thumbnail(&self, key: &str, data: &[u8]) -> Result<(), s3::error::S3Error> {
        let bucket = self.bucket.lock().await;
        bucket
            .put_object_with_content_type(key, data, "image/jpeg")
            .await?;
        Ok(())
    }

    pub async fn get_object(&self, key: &str) -> Result<Vec<u8>, s3::error::S3Error> {
        let bucket = self.bucket.lock().await;
        let resp = bucket.get_object(key).await?;
        Ok(resp.bytes().to_vec())
    }

    pub async fn get_object_range(
        &self,
        key: &str,
        start: u64,
        end: Option<u64>,
    ) -> Result<RangeResponse, s3::error::S3Error> {
        let bucket = self.bucket.lock().await;
        let resp = bucket.get_object_range(key, start, end).await?;

        let total_len = resp
            .headers()
            .get("content-range")
            .map(|v| v.as_str())
            .and_then(|v| {
                v.split('/').nth(1).and_then(|s| s.parse::<u64>().ok())
            })
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
