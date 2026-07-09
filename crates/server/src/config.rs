use crate::storage::S3Config;

pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub s3: Option<S3Config>,
    pub storage_mode: StorageMode,
}

pub enum StorageMode {
    S3,
    Local,
}

pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub base_url: String,
    pub upload_token: String,
}

pub struct DatabaseConfig {
    pub url: String,
}

impl Config {
    pub fn from_env_or_default() -> Self {
        let s3_endpoint = std::env::var("S3_ENDPOINT").ok();
        let s3_bucket = std::env::var("S3_BUCKET").ok();
        let s3_access_key = std::env::var("S3_ACCESS_KEY").ok();
        let s3_secret_key = std::env::var("S3_SECRET_KEY").ok();

        let storage_mode = if s3_endpoint.is_some()
            && s3_bucket.is_some()
            && s3_access_key.is_some()
            && s3_secret_key.is_some()
        {
            StorageMode::S3
        } else {
            StorageMode::Local
        };

        let s3 = match storage_mode {
            StorageMode::S3 => Some(S3Config {
                endpoint: s3_endpoint.unwrap(),
                region: std::env::var("S3_REGION").unwrap_or_else(|_| "garage".into()),
                bucket: s3_bucket.unwrap(),
                access_key: s3_access_key.unwrap(),
                secret_key: s3_secret_key.unwrap(),
            }),
            StorageMode::Local => None,
        };

        Config {
            server: ServerConfig {
                host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into()),
                port: std::env::var("PORT")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(8080),
                base_url: std::env::var("BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:8080".into()),
                upload_token: std::env::var("UPLOAD_TOKEN")
                    .unwrap_or_else(|_| "change-me-to-a-random-secret".into()),
            },
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL")
                    .unwrap_or_else(|_| "sqlite:data/db/medal-clone.db?mode=rwc".into()),
            },
            s3,
            storage_mode,
        }
    }
}