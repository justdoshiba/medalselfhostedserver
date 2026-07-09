use clap::Parser;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "medal-clone-watcher", version, about = "Watches Medal output folder and uploads clips to your server")]
struct Cli {
    #[arg(short, long, env = "WATCH_DIR")]
    watch_dir: Option<PathBuf>,

    #[arg(short = 'u', long = "server", env = "SERVER_URL", default_value = "http://localhost:8080")]
    server_url: String,

    #[arg(short, long, env = "UPLOAD_TOKEN")]
    upload_token: String,

    #[arg(long, help = "Install to Windows startup (Run key in Registry)")]
    install: bool,

    #[arg(long, help = "Remove from Windows startup")]
    uninstall: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.install {
        return install_startup().await;
    }
    if cli.uninstall {
        return uninstall_startup().await;
    }

    tracing_subscriber::fmt()
        .with_env_filter("medal_clone_watcher=info")
        .init();

    let watch_dir = cli.watch_dir.unwrap_or_else(|| {
        dirs::video_dir()
            .map(|p| p.join("Medal"))
            .unwrap_or_else(|| PathBuf::from("."))
    });

    info!(
        "Watching {} for new clips, uploading to {}",
        watch_dir.display(),
        cli.server_url
    );

    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;

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

async fn install_startup() -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let path = exe.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _disp) = hkcu.create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")?;
        key.set_value("MedalCloneWatcher", &path)?;
        println!("Installed to HKCU\\...\\Run: {}", path);
    }

    #[cfg(not(target_os = "windows"))]
    {
        println!("--install is only supported on Windows");
        println!("To auto-start on Linux/macOS, add this binary to your DE/WM autostart");
        println!("Binary path: {path}");
    }

    Ok(())
}

async fn uninstall_startup() -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run = hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            winreg::enums::KEY_WRITE,
        )?;
        match run.delete_value("MedalCloneWatcher") {
            Ok(_) => println!("Removed from startup"),
            Err(e) => println!("Not found in startup (already removed): {e}"),
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        println!("--uninstall is only supported on Windows");
    }

    Ok(())
}