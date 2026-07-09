use sqlx::sqlite::SqlitePool;

pub async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS clips (
            id          TEXT PRIMARY KEY NOT NULL,
            slug        TEXT NOT NULL UNIQUE,
            title       TEXT NOT NULL DEFAULT '',
            filename    TEXT NOT NULL,
            thumbnail_path TEXT,
            duration_secs   REAL,
            width       INTEGER,
            height      INTEGER,
            file_size_bytes INTEGER NOT NULL DEFAULT 0,
            created_at  TEXT NOT NULL,
            view_count  INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_clips_slug ON clips(slug)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_clips_created_at ON clips(created_at)",
    )
    .execute(pool)
    .await?;

    Ok(())
}
