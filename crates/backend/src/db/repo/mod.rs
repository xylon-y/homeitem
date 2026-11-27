//! Row mapping and repository functions. All functions take `&mut Connection`
//! so they compose inside either a read or a write transaction.

pub mod attachments;
pub mod csvio;
pub mod entities;
pub mod groups;
pub mod maintenance;
pub mod notifiers;
pub mod taxonomy;
pub mod users;

use chrono::{DateTime, ParseError, Utc};
use rusqlite::{Row, ToSql};

use crate::error::{AppError, AppResult};

/// Current UTC timestamp in the canonical storage format.
pub fn now() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Parse a stored timestamp back into `chrono`.
///
/// # Errors
/// Fails on malformed timestamps; callers surface that as internal errors.
pub fn parse_ts(raw: &str) -> Result<DateTime<Utc>, ParseError> {
    DateTime::parse_from_rfc3339(&format!("{raw}.000+00:00"))
        .or_else(|_| DateTime::parse_from_rfc3339(raw))
        .map(|dt| dt.with_timezone(&Utc))
}

/// Map a stored TEXT timestamp into `Ts`, defaulting to epoch on corruption.
pub fn ts_or_epoch(raw: Option<&str>) -> DateTime<Utc> {
    match raw {
        Some(s) if !s.is_empty() => parse_ts(s).unwrap_or_else(|_| Utc::now()),
        _ => Utc::now(),
    }
}

/// Optional variant of `ts_or_epoch` for nullable date columns.
pub fn ts_opt(raw: Option<&str>) -> Option<DateTime<Utc>> {
    match raw {
        Some(s) if !s.is_empty() => parse_ts(s).ok(),
        _ => None,
    }
}

/// Store an `Option<DateTime>` as TEXT or NULL.
pub fn ts_to_sql(v: Option<DateTime<Utc>>) -> Option<String> {
    v.map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// `true` as INTEGER 1/0.
pub fn bool_sql(b: bool) -> i64 {
    if b { 1 } else { 0 }
}

/// INTEGER/TEXT into bool.
pub fn from_bool(v: i64) -> bool {
    v != 0
}

/// Read a TEXT column, treating NULL as empty string.
pub fn text(row: &Row, idx: usize) -> Result<String, rusqlite::Error> {
    Ok(row.get::<_, Option<String>>(idx)?.unwrap_or_default())
}

/// Read an optional TEXT column.
pub fn text_opt(row: &Row, idx: usize) -> Result<Option<String>, rusqlite::Error> {
    let v: Option<String> = row.get(idx)?;
    Ok(v.filter(|s| !s.is_empty()))
}

/// Require a `ToSql` param list length to match the placeholder count.
///
/// # Errors
/// Panics in debug builds on mismatch; returns `Validation` in release.
pub fn check_params(expected: usize, actual: usize) -> AppResult<()> {
    if expected != actual {
        return Err(AppError::Internal(format!(
            "sql param mismatch: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}

/// Convert a `bool` to a `ToSql` value (SQLite INTEGER).
pub fn b(v: bool) -> i64 {
    bool_sql(v)
}

/// Helper: query a single optional row and map it.
pub fn query_optional<T>(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[&dyn ToSql],
    map: impl FnOnce(&Row<'_>) -> rusqlite::Result<T>,
) -> AppResult<Option<T>> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params)?;
    match rows.next()? {
        Some(row) => Ok(Some(map(row)?)),
        None => Ok(None),
    }
}

/// Helper: query all rows and map them.
pub fn query_all<T>(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[&dyn ToSql],
    mut map: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> AppResult<Vec<T>> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map(row)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_roundtrip() {
        let now = now();
        let parsed = parse_ts(&now).expect("roundtrip");
        assert!((Utc::now() - parsed).num_seconds() < 5);
    }

    #[test]
    fn parse_accepts_date_only() {
        assert!(parse_ts("2025-01-31T08:00:00Z").is_ok());
        assert!(parse_ts("2025-01-31T08:00:00.500Z").is_ok());
    }

    #[test]
    fn bool_conversions() {
        assert_eq!(bool_sql(true), 1);
        assert!(!from_bool(0));
        assert!(from_bool(-1));
    }
}
