use crate::storage::S3Config;

pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub s3: S3Config,
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
            s3: S3Config {
                endpoint: std::env::var("S3_ENDPOINT")
                    .expect("S3_ENDPOINT must be set"),
                region: std::env::var("S3_REGION")
                    .unwrap_or_else(|_| "garage".into()),
                bucket: std::env::var("S3_BUCKET")
                    .expect("S3_BUCKET must be set"),
                access_key: std::env::var("S3_ACCESS_KEY")
                    .expect("S3_ACCESS_KEY must be set"),
                secret_key: std::env::var("S3_SECRET_KEY")
                    .expect("S3_SECRET_KEY must be set"),
            },
        }
    }
}
