mod config;
mod db;
mod models;
mod routes;

use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};
use sqlx::sqlite::SqlitePoolOptions;
use tracing::info;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("medal_clone_server=info,actix_web=info")
        .init();

    dotenvy::dotenv().ok();

    let cfg = config::Config::from_env_or_default();
    info!("Starting server on {}:{}", cfg.server.host, cfg.server.port);
    info!("Base URL: {}", cfg.server.base_url);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&cfg.database.url)
        .await?;

    db::run_migrations(&pool).await?;
    info!("Database ready");

    let pool_data = web::Data::new(pool);
    let cfg_data = web::Data::new(cfg.clone());

    let bind = format!("{}:{}", cfg.server.host, cfg.server.port);

    HttpServer::new(move || {
        App::new()
            .app_data(pool_data.clone())
            .app_data(cfg_data.clone())
            .wrap(Logger::default())
            .configure(routes::configure)
    })
    .bind(&bind)?
    .run()
    .await?;

    Ok(())
}
