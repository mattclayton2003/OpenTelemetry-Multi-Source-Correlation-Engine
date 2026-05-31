use sqlx::sqlite::SqlitePool;

pub async fn open(_path: &std::path::Path) -> anyhow::Result<SqlitePool> {
    // Filled in Task 6.2.
    anyhow::bail!("not yet implemented")
}
