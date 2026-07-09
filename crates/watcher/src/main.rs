use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::{error, info};

struct Cli {
    watch_dir: PathBuf,
    server_url: String,
    upload_token: String,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("medal_clone_watcher=info")
        .init();

    // TODO: parse CLI args / config file for watch dir, server url, token
    let cli = Cli {
        watch_dir: PathBuf::from(std::env::var("WATCH_DIR").unwrap_or_else(|_| ".".into())),
        server_url: std::env::var("SERVER_URL")
            .unwrap_or_else(|_| "http://localhost:8080".into()),
        upload_token: std::env::var("UPLOAD_TOKEN")
            .unwrap_or_else(|_| "change-me".into()),
    };

    info!(
        "Watching {} → {}",
        cli.watch_dir.display(),
        cli.server_url
    );

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&cli.watch_dir, RecursiveMode::NonRecursive)?;

    for event in rx {
        match event {
            Ok(event) => {
                if let EventKind::Create(_) | EventKind::Modify(_) = event.kind {
                    for path in event.paths {
                        if path
                            .extension()
                            .is_some_and(|ext| ext == "mp4" || ext == "mov")
                        {
                            info!("New clip detected: {}", path.display());
                            // TODO: wait for file stability, then upload via reqwest
                        }
                    }
                }
            }
            Err(e) => error!("Watch error: {e}"),
        }
    }

    Ok(())
}
