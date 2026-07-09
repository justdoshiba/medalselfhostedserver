use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub base_url: String,
    pub data_dir: String,
    pub upload_token: String,
}

#[derive(Debug, Clone, Deserialize)]
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
                data_dir: std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into()),
                upload_token: std::env::var("UPLOAD_TOKEN")
                    .unwrap_or_else(|_| "change-me-to-a-random-secret".into()),
            },
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL")
                    .unwrap_or_else(|_| "sqlite:data/db/medal-clone.db?mode=rwc".into()),
            },
        }
    }
}
