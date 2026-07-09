mod config;
mod db;
mod models;
mod routes;
mod storage;

use actix_cors::Cors;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use tracing::info;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("medal_clone_server=info,actix_web=info")
        .init();

    dotenvy::dotenv().ok();

    let cfg = config::Config::from_env_or_default();
    info!(
        "Starting server on {}:{}",
        cfg.server.host, cfg.server.port
    );
    info!("Base URL: {}", cfg.server.base_url);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&cfg.database.url)
        .await?;

    db::run_migrations(&pool).await?;
    info!("Database ready");

    let storage = match cfg.storage_mode {
        config::StorageMode::S3 => {
            let s3_cfg = cfg.s3.as_ref().unwrap();
            info!(
                "Storage: S3 (Garage) at {} bucket {}",
                s3_cfg.endpoint, s3_cfg.bucket
            );
            storage::Storage::from_config(s3_cfg)
        }
        config::StorageMode::Local => {
            let data_dir = PathBuf::from(
                std::env::var("DATA_DIR").unwrap_or_else(|_| "data/storage".into()),
            );
            info!("Storage: local filesystem at {}", data_dir.display());
            storage::Storage::local(data_dir)
        }
    };

    let pool_data = web::Data::new(pool);
    let cfg_data = web::Data::new(cfg);
    let storage_data = web::Data::new(storage);

    let bind = format!("{}:{}", cfg_data.server.host, cfg_data.server.port);

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .app_data(pool_data.clone())
            .app_data(cfg_data.clone())
            .app_data(storage_data.clone())
            .wrap(Logger::default())
            .configure(routes::configure)
    })
    .bind(&bind)?
    .run()
    .await?;

    Ok(())
}