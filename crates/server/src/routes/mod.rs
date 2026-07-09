mod clips;
mod embed;
mod upload;

use actix_web::HttpResponse;

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.route("/health", actix_web::web::get().to(health))
        .service(
            actix_web::web::scope("/api")
                .route("/upload", actix_web::web::post().to(upload::upload_clip))
                .route("/clips/{slug}", actix_web::web::get().to(clips::get_clip))
                .route(
                    "/clips/{slug}/video",
                    actix_web::web::get().to(clips::serve_video),
                )
                .route(
                    "/clips/{slug}/thumb",
                    actix_web::web::get().to(clips::serve_thumbnail),
                ),
        )
        .route("/clip/{slug}", actix_web::web::get().to(embed::clip_page))
        .route(
            "/clip/{slug}/embed",
            actix_web::web::get().to(embed::embed_iframe),
        )
        .route("/oembed", actix_web::web::get().to(embed::oembed));
}
