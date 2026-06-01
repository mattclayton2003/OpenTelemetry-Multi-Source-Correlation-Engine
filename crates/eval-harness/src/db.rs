use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteConnectOptions, SqliteJournalMode};
use std::str::FromStr;
use std::time::Duration;

pub async fn open(path: &std::path::Path) -> anyhow::Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new().max_connections(4).connect_with(opts).await?;
    Ok(pool)
}
