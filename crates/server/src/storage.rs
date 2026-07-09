use std::path::PathBuf;

use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::{Digest, Sha256};

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
    client: Client,
    cfg: S3Config,
}

pub struct LocalStorage {
    data_dir: PathBuf,
}

impl S3Storage {
    fn s3_url(&self, key: &str) -> String {
        format!("{}/{}/{}", self.cfg.endpoint.trim_end_matches('/'), self.cfg.bucket, key)
    }

    async fn s3_send(&self, method: &str, key: &str, body: &[u8], range: Option<&str>) -> reqwest::Result<reqwest::Response> {
        let url = self.s3_url(key);
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let hashed_payload = hex::encode(sha256_hash(body));
        let content_length = body.len().to_string();

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Host", reqwest::header::HeaderValue::from_str(&url::Url::parse(&url).unwrap().host_str().unwrap()).unwrap());
        headers.insert("x-amz-date", reqwest::header::HeaderValue::from_str(&amz_date).unwrap());
        headers.insert("x-amz-content-sha256", reqwest::header::HeaderValue::from_str(&hashed_payload).unwrap());
        headers.insert("Content-Length", reqwest::header::HeaderValue::from_str(&content_length).unwrap());

        let mut signed_headers = vec!["content-length", "host", "x-amz-content-sha256", "x-amz-date"];
        if range.is_some() {
            headers.insert("Range", reqwest::header::HeaderValue::from_str(range.unwrap()).unwrap());
            signed_headers.insert(0, "range");
        }

        let canonical_uri = format!("/{}/{}", self.cfg.bucket, key);
        let canonical_querystring = "";
        let signed_headers_str = signed_headers.join(";");
        let canonical_headers: String = signed_headers.iter().map(|h| {
            let val = headers.get(*h).map(|v| v.to_str().unwrap()).unwrap_or("");
            format!("{}:{}\n", h, val)
        }).collect();

        let canonical_request = format!("{method}\n{canonical_uri}\n{canonical_querystring}\n{canonical_headers}\n{signed_headers_str}\n{hashed_payload}");

        let algorithm = "AWS4-HMAC-SHA256";
        let credential_scope = format!("{date_stamp}/{}/s3/aws4_request", self.cfg.region);
        let string_to_sign = format!("{algorithm}\n{amz_date}\n{credential_scope}\n{}", hex::encode(sha256_hash(canonical_request.as_bytes())));

        let signing_key = derive_signing_key(&self.cfg.secret_key, &date_stamp, &self.cfg.region, "s3");
        let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

        let auth_header = format!("{algorithm} Credential={}/{}, SignedHeaders={signed_headers_str}, Signature={signature}", self.cfg.access_key, credential_scope);
        headers.insert("Authorization", reqwest::header::HeaderValue::from_str(&auth_header).unwrap());

        let client = Client::new();
        let req = match method {
            "PUT" => client.put(&url).headers(headers).body(body.to_vec()),
            "DELETE" => client.delete(&url).headers(headers),
            _ => {
                let mut r = client.get(&url).headers(headers);
                if let Some(range_val) = range {
                    r = r.header("Range", range_val);
                }
                r
            }
        };

        req.send().await
    }
}

fn sha256_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn derive_signing_key(secret: &str, date_stamp: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

impl Storage {
    pub fn from_config(cfg: &S3Config) -> Self {
        Storage::S3(S3Storage {
            client: Client::new(),
            cfg: cfg.clone(),
        })
    }

    pub fn local(data_dir: PathBuf) -> Self {
        Storage::Local(LocalStorage { data_dir })
    }

    pub async fn put_video(&self, key: &str, data: &[u8]) -> anyhow::Result<()> {
        match self {
            Storage::S3(s) => {
                let resp = s.s3_send("PUT", key, data, None).await?;
                if !resp.status().is_success() {
                    anyhow::bail!("s3 put failed: HTTP {}", resp.status());
                }
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
                let resp = s.s3_send("PUT", key, data, None).await?;
                if !resp.status().is_success() {
                    anyhow::bail!("s3 put thumbnail failed: HTTP {}", resp.status());
                }
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
                let resp = s.s3_send("GET", key, &[], None).await?;
                if !resp.status().is_success() {
                    anyhow::bail!("s3 get failed: HTTP {}", resp.status());
                }
                Ok(resp.bytes().await?.to_vec())
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
                let range_str = match end {
                    Some(e) => format!("bytes={}-{}", start, e),
                    None => format!("bytes={}-", start),
                };
                let resp = s.s3_send("GET", key, &[], Some(&range_str)).await?;
                if !resp.status().is_success() {
                    anyhow::bail!("s3 range get failed: HTTP {}", resp.status());
                }

                let content_range = resp
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let content_length = resp
                    .headers()
                    .get("content-length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);

                Ok(RangeResponse {
                    data: resp.bytes().await?.to_vec(),
                    content_length,
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
                let resp = s.s3_send("DELETE", key, &[], None).await?;
                if !resp.status().is_success() {
                    anyhow::bail!("s3 delete failed: HTTP {}", resp.status());
                }
                Ok(())
            }
            Storage::Local(l) => {
                let path = l.data_dir.join("videos").join(key);
                let _ = tokio::fs::remove_file(&path).await;
                let _ = tokio::fs::remove_file(&path).await;
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