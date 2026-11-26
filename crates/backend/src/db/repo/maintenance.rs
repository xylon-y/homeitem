//! Maintenance / service log entries.

use common::dto::{MaintenanceCreate, MaintenanceEntry, MaintenanceUpdate};
use rusqlite::{params, Connection};

use super::{query_all, query_optional, text, ts_opt, ts_or_epoch};
use crate::error::{AppError, AppResult};

const COLS: &str = "id, entity_id, name, description, cost, completed_date, scheduled_date, created_at, updated_at";

fn map(row: &rusqlite::Row<'_>) -> rusqlite::Result<MaintenanceEntry> {
    Ok(MaintenanceEntry {
        id: text(row, 0)?,
        entity_id: text(row, 1)?,
        name: text(row, 2)?,
        description: text(row, 3)?,
        cost: row.get(4)?,
        completed_date: ts_opt(row.get::<_, Option<String>>(5)?.as_deref()),
        scheduled_date: ts_opt(row.get::<_, Option<String>>(6)?.as_deref()),
        created_at: ts_or_epoch(row.get::<_, Option<String>>(7)?.as_deref()),
        updated_at: ts_or_epoch(row.get::<_, Option<String>>(8)?.as_deref()),
    })
}

/// Entries of one entity.
///
/// `status`: "all" (default), "scheduled" (future / not completed) or
/// "completed".
pub fn list(conn: &Connection, entity_id: &str, status: &str) -> AppResult<Vec<MaintenanceEntry>> {
    let mut sql = format!("SELECT {COLS} FROM maintenance_entries WHERE entity_id = ?1");
    match status {
        "scheduled" => sql.push_str(" AND completed_date IS NULL ORDER BY scheduled_date"),
        "completed" => sql.push_str(" AND completed_date IS NOT NULL ORDER BY completed_date DESC"),
        _ => sql.push_str(" ORDER BY scheduled_date IS NULL, scheduled_date, created_at DESC"),
    }
    query_all(conn, &sql, params![entity_id], map)
}

/// Create an entry; returns its id.
pub fn create(conn: &Connection, entity_id: &str, req: &MaintenanceCreate) -> AppResult<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = super::now();
    conn.execute(
        "INSERT INTO maintenance_entries (id, entity_id, name, description, cost,
                                          completed_date, scheduled_date, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            id,
            entity_id,
            req.name,
            req.description,
            req.cost,
            super::ts_to_sql(req.completed_date),
            super::ts_to_sql(req.scheduled_date),
            now
        ],
    )?;
    Ok(id)
}

/// Update an entry (entity-scoped).
pub fn update(
    conn: &Connection,
    entity_id: &str,
    id: &str,
    req: &MaintenanceUpdate,
) -> AppResult<()> {
    let now = super::now();
    let n = conn.execute(
        "UPDATE maintenance_entries SET name = ?1, description = ?2, cost = ?3,
                                        completed_date = ?4, scheduled_date = ?5, updated_at = ?6
          WHERE id = ?7 AND entity_id = ?8",
        params![
            req.name,
            req.description,
            req.cost,
            super::ts_to_sql(req.completed_date),
            super::ts_to_sql(req.scheduled_date),
            now,
            id,
            entity_id
        ],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("保养记录不存在".into()));
    }
    Ok(())
}

/// Delete an entry.
pub fn delete(conn: &Connection, entity_id: &str, id: &str) -> AppResult<()> {
    let n = conn.execute(
        "DELETE FROM maintenance_entries WHERE id = ?1 AND entity_id = ?2",
        params![id, entity_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("保养记录不存在".into()));
    }
    Ok(())
}

/// One entry by id (entity-scoped), for reminders that need the entity name.
pub fn find(conn: &Connection, entity_id: &str, id: &str) -> AppResult<Option<MaintenanceEntry>> {
    query_optional(
        conn,
        &format!("SELECT {COLS} FROM maintenance_entries WHERE id = ?1 AND entity_id = ?2"),
        &[&id, &entity_id],
        map,
    )
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
    fn lifecycle() {
        let conn = setup();
        let id = create(
            &conn,
            "it1",
            &MaintenanceCreate {
                name: "换滤芯".into(),
                description: String::new(),
                cost: 30.0,
                completed_date: None,
                scheduled_date: chrono::DateTime::parse_from_rfc3339("2026-06-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc)
                    .into(),
            },
        )
        .unwrap();
        let all = list(&conn, "it1", "all").unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(list(&conn, "it1", "scheduled").unwrap().len(), 1);
        assert_eq!(list(&conn, "it1", "completed").unwrap().len(), 0);

        update(
            &conn,
            "it1",
            &id,
            &MaintenanceUpdate {
                name: "换滤芯2".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(list(&conn, "it1", "all").unwrap()[0].name, "换滤芯2");

        delete(&conn, "it1", &id).unwrap();
        assert!(list(&conn, "it1", "all").unwrap().is_empty());
        assert!(delete(&conn, "it1", &id).is_err());
    }
}
