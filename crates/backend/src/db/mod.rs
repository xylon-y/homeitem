//! SQLite access layer: r2d2 pool + spawn_blocking wrappers.

pub mod migrations;
pub mod repo;

use std::path::Path;

use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

use crate::config::Config;
use crate::error::{AppError, AppResult};

/// Shared connection pool type.
pub type Pool = r2d2::Pool<SqliteConnectionManager>;

/// Open the database, apply migrations and build the pool.
///
/// # Errors
/// Returns `AppError::Internal` when the file cannot be opened or migrated.
pub fn init(config: &Config) -> AppResult<Pool> {
    if let Some(dir) = config.db_path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| AppError::Internal(format!("create db dir: {e}")))?;
    }
    let manager = SqliteConnectionManager::file(&config.db_path)
        .with_init(|c| {
            c.execute_batch(
                "PRAGMA foreign_keys = ON;
                 PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA busy_timeout = 5000;",
            )
        });
    let pool = r2d2::Pool::builder().max_size(8).build(manager)?;
    migrations::run(&pool)?;
    Ok(pool)
}

/// Run a write closure inside a transaction on a pooled connection.
///
/// All blocking SQLite work is moved off the async runtime via
/// `tokio::spawn_blocking`. Closures receive `&Connection` — rusqlite's
/// statement methods only need `&self`, and `Transaction` derefs to
/// `Connection`.
pub async fn with_write<T, F>(pool: &Pool, f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> AppResult<T> + Send + 'static,
{
    let pool = pool.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut conn = pool.get()?;
        let tx = conn.transaction()?;
        let value = f(&tx)?;
        tx.commit()?;
        Ok(value)
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join: {e}")))?;
    out
}

/// Run a read-only closure on a pooled connection (no transaction).
pub async fn with_read<T, F>(pool: &Pool, f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> AppResult<T> + Send + 'static,
{
    let pool = pool.clone();
    let out = tokio::task::spawn_blocking(move || {
        let conn = pool.get()?;
        f(&conn)
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join: {e}")))?;
    out
}

/// Ensure the upload directory exists.
pub fn init_upload_dir(config: &Config) -> AppResult<()> {
    std::fs::create_dir_all(&config.upload_dir)
        .map_err(|e| AppError::Internal(format!("create upload dir: {e}")))?;
    Ok(())
}

/// Resolve a path and guarantee it stays inside `base` (path traversal guard).
pub fn safe_join(base: &Path, relative: &str) -> AppResult<std::path::PathBuf> {
    let joined = base.join(relative);
    let base_canon = base
        .canonicalize()
        .map_err(|e| AppError::Internal(format!("canonicalize base: {e}")))?;
    // The file may not exist yet; canonicalize the parent instead.
    let parent = joined
        .parent()
        .ok_or_else(|| AppError::Internal("bad relative path".into()))?;
    let parent_canon = parent
        .canonicalize()
        .map_err(|_| AppError::BadRequest("invalid attachment path".into()))?;
    if !parent_canon.starts_with(&base_canon) {
        return Err(AppError::Forbidden("attachment path escapes upload dir".into()));
    }
    let name = joined
        .file_name()
        .ok_or_else(|| AppError::BadRequest("invalid attachment name".into()))?;
    Ok(parent_canon.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_blocks_escape() {
        let dir = std::env::temp_dir().join("homeitem_safe_join_test");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let ok = safe_join(&dir, "sub/file.txt");
        assert!(ok.is_ok());
        let bad = safe_join(&dir, "../outside.txt");
        assert!(bad.is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
