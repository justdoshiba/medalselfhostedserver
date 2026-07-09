use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

struct Cli {
    watch_dir: PathBuf,
    server_url: String,
    upload_token: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("medal_clone_watcher=info")
        .init();

    let cli = Cli {
        watch_dir: PathBuf::from(
            std::env::var("WATCH_DIR")
                .unwrap_or_else(|_| dirs::video_dir()
                    .map(|p| p.join("Medal"))
                    .unwrap_or_else(|| PathBuf::from("."))
                    .to_string_lossy()
                    .to_string()),
        ),
        server_url: std::env::var("SERVER_URL")
            .unwrap_or_else(|_| "http://localhost:8080".into()),
        upload_token: std::env::var("UPLOAD_TOKEN")
            .unwrap_or_else(|_| "change-me".into()),
    };

    info!(
        "Watching {} for new clips, uploading to {}",
        cli.watch_dir.display(),
        cli.server_url
    );

    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&cli.watch_dir, RecursiveMode::NonRecursive)?;

    let upload_url = format!("{}/api/upload", cli.server_url);
    let token = cli.upload_token.clone();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;

    for event in rx {
        let event = match event {
            Ok(e) => e,
            Err(e) => {
                error!("Watch error: {e}");
                continue;
            }
        };

        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {}
            _ => continue,
        }

        for path in event.paths {
            let ext = match path.extension().and_then(|e| e.to_str()) {
                Some(e) if e == "mp4" || e == "mov" || e == "webm" => e.to_string(),
                _ => continue,
            };

            info!("New clip detected: {}", path.display());

            if !wait_for_stability(&path).await {
                warn!("File never stabilized, skipping: {}", path.display());
                continue;
            }

            let metadata = match tokio::fs::metadata(&path).await {
                Ok(m) => m,
                Err(e) => {
                    warn!("Cannot stat file (may have been deleted): {e}");
                    continue;
                }
            };

            if metadata.len() == 0 {
                warn!("Empty file, skipping: {}", path.display());
                continue;
            }

            let mut attempt = 0;
            let max_attempts = 3;

            loop {
                attempt += 1;

                let file_bytes = match tokio::fs::read(&path).await {
                    Ok(b) => b,
                    Err(e) => {
                        error!("Failed to read file for upload: {e}");
                        break;
                    }
                };
                let mime = match ext.as_str() {
                    "mp4" => "video/mp4",
                    "mov" => "video/quicktime",
                    "webm" => "video/webm",
                    _ => "video/mp4",
                };
                let file_part = reqwest::multipart::Part::bytes(file_bytes)
                    .file_name(format!("clip.{}", ext))
                    .mime_str(mime)
                    .unwrap();

                let form = reqwest::multipart::Form::new()
                    .part("file", file_part);

                let result = client
                    .post(&upload_url)
                    .header("Authorization", format!("Bearer {}", token))
                    .multipart(form)
                    .send()
                    .await;

                match result {
                    Ok(resp) if resp.status().is_success() => {
                        info!("Uploaded {} successfully", path.display());
                        if let Err(e) = tokio::fs::remove_file(&path).await {
                            warn!("Failed to delete uploaded file: {e}");
                        } else {
                            info!("Deleted local copy: {}", path.display());
                        }
                        break;
                    }
                    Ok(resp) => {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        warn!(
                            "Upload failed (attempt {}/{}): HTTP {} - {}",
                            attempt, max_attempts, status, body
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Upload error (attempt {}/{}): {e}",
                            attempt, max_attempts
                        );
                    }
                }

                if attempt >= max_attempts {
                    error!("All upload attempts failed for: {}", path.display());
                    break;
                }

                tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
            }
        }
    }

    Ok(())
}

async fn wait_for_stability(path: &PathBuf) -> bool {
    let max_wait = Duration::from_secs(30);
    let check_interval = Duration::from_millis(500);
    let start = Instant::now();

    let mut prev_size = match tokio::fs::metadata(path).await {
        Ok(m) => m.len(),
        Err(_) => return false,
    };

    while start.elapsed() < max_wait {
        tokio::time::sleep(check_interval).await;

        match tokio::fs::metadata(path).await {
            Ok(m) => {
                let size = m.len();
                if size == prev_size && size > 0 {
                    return true;
                }
                prev_size = size;
            }
            Err(_) => return false,
        }
    }

    false
}
