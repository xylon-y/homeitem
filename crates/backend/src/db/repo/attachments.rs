//! Attachment metadata + file storage helpers.

use common::dto::ItemAttachment;
use rusqlite::{params, Connection};

use super::{bool_sql, from_bool, query_all, query_optional, text, ts_or_epoch};
use crate::error::{AppError, AppResult};

const COLS: &str = "id, entity_id, type, title, file_name, path, mime_type, size_bytes, primary_photo, created_at, updated_at";

fn map(row: &rusqlite::Row<'_>) -> rusqlite::Result<ItemAttachment> {
    Ok(ItemAttachment {
        id: text(row, 0)?,
        kind: text(row, 2)?,
        title: text(row, 3)?,
        file_name: text(row, 4)?,
        path: text(row, 5)?,
        mime_type: text(row, 6)?,
        size_bytes: row.get(7)?,
        primary: from_bool(row.get(8)?),
        created_at: ts_or_epoch(row.get::<_, Option<String>>(9)?.as_deref()),
        updated_at: ts_or_epoch(row.get::<_, Option<String>>(10)?.as_deref()),
    })
}

/// All attachments of an entity, primary photo first.
pub fn list(conn: &Connection, entity_id: &str) -> AppResult<Vec<ItemAttachment>> {
    query_all(
        conn,
        &format!("SELECT {COLS} FROM attachments WHERE entity_id = ?1 ORDER BY primary_photo DESC, created_at"),
        params![entity_id],
        map,
    )
}

/// One attachment by id (entity-scoped).
pub fn find(conn: &Connection, entity_id: &str, id: &str) -> AppResult<Option<ItemAttachment>> {
    query_optional(
        conn,
        &format!("SELECT {COLS} FROM attachments WHERE entity_id = ?1 AND id = ?2"),
        &[&entity_id, &id],
        map,
    )
}

/// Attachment metadata by id only (media endpoint).
pub fn find_any(conn: &Connection, id: &str) -> AppResult<Option<ItemAttachment>> {
    query_optional(
        conn,
        &format!("SELECT {COLS} FROM attachments WHERE id = ?1"),
        &[&id],
        map,
    )
}

/// Insert attachment metadata. `primary` demotes any existing primary photo.
pub fn insert(
    conn: &Connection,
    id: &str,
    entity_id: &str,
    kind: &str,
    title: &str,
    file_name: &str,
    path: &str,
    mime_type: &str,
    size_bytes: i64,
    primary: bool,
) -> AppResult<()> {
    let now = super::now();
    if primary {
        demote_primary(conn, entity_id)?;
    }
    conn.execute(
        "INSERT INTO attachments (id, entity_id, type, title, file_name, path, mime_type,
                                  size_bytes, primary_photo, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![id, entity_id, kind, title, file_name, path, mime_type, size_bytes, bool_sql(primary), now],
    )?;
    Ok(())
}

fn demote_primary(conn: &Connection, entity_id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE attachments SET primary_photo = 0, updated_at = ?1 WHERE entity_id = ?2 AND primary_photo = 1",
        params![super::now(), entity_id],
    )?;
    Ok(())
}

/// Update title/type/primary flag. Returns the stored row.
pub fn update(
    conn: &Connection,
    entity_id: &str,
    id: &str,
    title: &str,
    kind: &str,
    primary: bool,
) -> AppResult<ItemAttachment> {
    let now = super::now();
    if primary {
        demote_primary(conn, entity_id)?;
    }
    let n = conn.execute(
        "UPDATE attachments SET title = ?1, type = ?2, primary_photo = ?3, updated_at = ?4
          WHERE id = ?5 AND entity_id = ?6",
        params![title, kind, bool_sql(primary), now, id, entity_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("附件不存在".into()));
    }
    find(conn, entity_id, id)?.ok_or_else(|| AppError::NotFound("附件不存在".into()))
}

/// Delete metadata; returns the relative file path for disk cleanup.
pub fn delete(conn: &Connection, entity_id: &str, id: &str) -> AppResult<String> {
    let path: Option<String> = query_optional(
        conn,
        "SELECT path FROM attachments WHERE id = ?1 AND entity_id = ?2",
        &[&id, &entity_id],
        |r| text(r, 0),
    )?;
    let path = path.ok_or_else(|| AppError::NotFound("附件不存在".into()))?;
    conn.execute(
        "DELETE FROM attachments WHERE id = ?1 AND entity_id = ?2",
        params![id, entity_id],
    )?;
    Ok(path)
}

/// Count photos used to compute `hasPhoto` filters in stats.
pub fn count_photos(conn: &Connection, group_id: &str) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM attachments a JOIN entities e ON e.id = a.entity_id
          WHERE e.group_id = ?1 AND a.primary_photo = 1",
        params![group_id],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn setup() -> r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager> {
        let manager = r2d2_sqlite::SqliteConnectionManager::memory();
        let pool = r2d2::Pool::builder().max_size(1).build(manager).unwrap();
        migrations::run(&pool).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO groups (id, name, currency, created_at, updated_at)
             VALUES ('g1', 'H', 'CNY', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let (loc, _) = super::super::taxonomy::seed_defaults(&conn, "g1").unwrap();
        conn.execute(
            "INSERT INTO entities (id, group_id, name, entity_type_id, created_at, updated_at)
             VALUES ('it1', 'g1', 'x', ?1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![loc],
        )
        .unwrap();
        conn
    }

    #[test]
    fn insert_list_update_delete() {
        let conn = setup();
        insert(&conn, "a1", "it1", "photo", "正面", "p.jpg", "it1/a1.jpg", "image/jpeg", 123, true)
            .unwrap();
        insert(&conn, "a2", "it1", "photo", "背面", "b.jpg", "it1/a2.jpg", "image/jpeg", 45, true)
            .unwrap();
        let all = list(&conn, "it1").unwrap();
        assert_eq!(all.len(), 2);
        // only one primary: a2
        let primaries = all.iter().filter(|a| a.primary).count();
        assert_eq!(primaries, 1);
        assert_eq!(all[0].id, "a2");

        let upd = update(&conn, "it1", "a1", "改名", "manual", false).unwrap();
        assert_eq!(upd.title, "改名");
        assert_eq!(upd.kind, "manual");

        let path = delete(&conn, "it1", "a1").unwrap();
        assert_eq!(path, "it1/a1.jpg");
        assert_eq!(list(&conn, "it1").unwrap().len(), 1);
        assert!(delete(&conn, "it1", "a1").is_err());
    }

    #[test]
    fn media_lookup_is_global() {
        let conn = setup();
        insert(&conn, "a1", "it1", "photo", "", "p.jpg", "it1/a1.jpg", "image/jpeg", 1, false).unwrap();
        assert!(find_any(&conn, "a1").unwrap().is_some());
        assert!(find_any(&conn, "zz").unwrap().is_none());
    }
}
