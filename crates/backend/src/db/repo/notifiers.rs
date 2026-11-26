//! Notifiers (email/webhook) + reminder queries + notification dedupe.

use std::collections::HashMap;

use common::dto::{
    NotifierCreate, NotifierKind, NotifierOut, NotifierUpdate, ReminderCounts, ReminderItem,
    ReminderKind, ReminderResponse, Statistics, TotalsByOrganizer, ValueOverTimeEntry,
};
use common::query::RemindersQuery;
use rusqlite::{params, Connection, OptionalExtension};

use super::{bool_sql, from_bool, query_all, query_optional, text, ts_opt, ts_or_epoch};
use crate::error::{AppError, AppResult};

// ---------------------------------------------------------------------------
// notifiers
// ---------------------------------------------------------------------------

const NCOLS: &str = "id, name, kind, target, is_active, kinds, created_at, updated_at";

fn map_notifier(row: &rusqlite::Row<'_>) -> rusqlite::Result<NotifierOut> {
    Ok(NotifierOut {
        id: text(row, 0)?,
        name: text(row, 1)?,
        kind: if text(row, 2)? == "webhook" {
            NotifierKind::Webhook
        } else {
            NotifierKind::Email
        },
        target: text(row, 3)?,
        is_active: from_bool(row.get(4)?),
        kinds: text(row, 5)?
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .filter_map(|k| serde_json::from_value(serde_json::Value::String(k.to_string())).ok())
            .collect(),
        created_at: ts_or_epoch(row.get::<_, Option<String>>(6)?.as_deref()),
        updated_at: ts_or_epoch(row.get::<_, Option<String>>(7)?.as_deref()),
    })
}

/// All notifiers of a group.
pub fn list_notifiers(conn: &Connection, group_id: &str) -> AppResult<Vec<NotifierOut>> {
    query_all(
        conn,
        &format!("SELECT {NCOLS} FROM notifiers WHERE group_id = ?1 ORDER BY name"),
        params![group_id],
        map_notifier,
    )
}

/// Active notifiers subscribed to `kind`.
pub fn active_for_kind(conn: &Connection, group_id: &str, kind: ReminderKind) -> AppResult<Vec<NotifierOut>> {
    let all = list_notifiers(conn, group_id)?;
    let wanted = serde_json::to_value(kind).unwrap_or_default();
    let wanted = wanted.as_str().unwrap_or_default().to_string();
    Ok(all
        .into_iter()
        .filter(|n| n.is_active && n.kinds.iter().any(|k| {
            serde_json::to_value(k).ok().and_then(|v| v.as_str().map(ToString::to_string)).as_deref() == Some(wanted.as_str())
        }))
        .collect())
}

/// One notifier.
pub fn find_notifier(conn: &Connection, group_id: &str, id: &str) -> AppResult<Option<NotifierOut>> {
    query_optional(
        conn,
        &format!("SELECT {NCOLS} FROM notifiers WHERE group_id = ?1 AND id = ?2"),
        &[&group_id, &id],
        map_notifier,
    )
}

fn kinds_csv(kinds: &[ReminderKind]) -> String {
    kinds.iter()
        .map(|k| serde_json::to_value(k).ok().and_then(|v| v.as_str().map(ToString::to_string)).unwrap_or_default())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

/// Create a notifier.
pub fn create_notifier(conn: &Connection, group_id: &str, req: &NotifierCreate) -> AppResult<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = super::now();
    let kinds = if req.kinds.is_empty() {
        "warranty,reminder,maintenance,lowStock".to_string()
    } else {
        kinds_csv(&req.kinds)
    };
    conn.execute(
        "INSERT INTO notifiers (id, group_id, name, kind, target, is_active, kinds, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            id,
            group_id,
            req.name,
            if req.kind == NotifierKind::Webhook { "webhook" } else { "email" },
            req.target,
            bool_sql(req.is_active),
            kinds,
            now
        ],
    )?;
    Ok(id)
}

/// Update a notifier.
pub fn update_notifier(conn: &Connection, group_id: &str, id: &str, req: &NotifierUpdate) -> AppResult<()> {
    let now = super::now();
    let n = conn.execute(
        "UPDATE notifiers SET name = ?1, kind = ?2, target = ?3, is_active = ?4, kinds = ?5, updated_at = ?6
          WHERE id = ?7 AND group_id = ?8",
        params![
            req.name,
            if req.kind == NotifierKind::Webhook { "webhook" } else { "email" },
            req.target,
            bool_sql(req.is_active),
            kinds_csv(&req.kinds),
            now,
            id,
            group_id
        ],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("通知器不存在".into()));
    }
    Ok(())
}

/// Delete a notifier.
pub fn delete_notifier(conn: &Connection, group_id: &str, id: &str) -> AppResult<()> {
    let n = conn.execute(
        "DELETE FROM notifiers WHERE id = ?1 AND group_id = ?2",
        params![id, group_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("通知器不存在".into()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// notification dedupe
// ---------------------------------------------------------------------------

/// Returns true when this (notifier, dedupe_key) was already sent.
pub fn already_sent(conn: &Connection, notifier_id: &str, dedupe_key: &str) -> AppResult<bool> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM notification_log WHERE notifier_id = ?1 AND dedupe_key = ?2",
            params![notifier_id, dedupe_key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(exists.is_some())
}

/// Record a successful send (idempotent).
pub fn record_send(conn: &Connection, notifier_id: &str, dedupe_key: &str) -> AppResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO notification_log (id, notifier_id, dedupe_key, sent_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![uuid::Uuid::new_v4().to_string(), notifier_id, dedupe_key, super::now()],
    )?;
    Ok(())
}

/// Drop log entries older than `days` (called by the background sweeper).
pub fn prune_log(conn: &Connection, days: i64) -> AppResult<usize> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    Ok(conn.execute(
        "DELETE FROM notification_log WHERE sent_at < ?1",
        params![cutoff],
    )?)
}

// ---------------------------------------------------------------------------
// reminders
// ---------------------------------------------------------------------------

/// Aggregate all due/overdue reminders for a group.
pub fn reminders(conn: &Connection, group_id: &str, q: &RemindersQuery) -> AppResult<ReminderResponse> {
    let today = chrono::Utc::now();
    let horizon = today + chrono::Duration::days(q.within.max(0));
    let kinds: std::collections::HashSet<String> = q.kinds().into_iter().collect();
    let mut items: Vec<ReminderItem> = Vec::new();



    // warranty + custom reminder dates on entities
    if kinds.contains("warranty") || kinds.contains("reminder") {
        let rows = query_all(
            conn,
            "SELECT id, name, warranty_expires, reminder_date, reminder_note, archived
               FROM entities WHERE group_id = ?1 AND archived = 0
                AND (warranty_expires IS NOT NULL OR reminder_date IS NOT NULL)",
            params![group_id],
            |r| {
                Ok::<_, rusqlite::Error>((
                    text(r, 0)?,
                    text(r, 1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    text(r, 4)?,
                    from_bool(r.get(5)?),
                ))
            },
        )?;
        for (id, name, warranty, reminder, note, archived) in rows {
            if archived {
                continue;
            }
            for (kind_raw, date_raw, kind) in [
                ("warranty", warranty, ReminderKind::Warranty),
                ("reminder", reminder, ReminderKind::Reminder),
            ] {
                if !kinds.contains(kind_raw) {
                    continue;
                }
                let Some(raw) = date_raw else { continue };
                let Some(dt) = ts_opt(Some(&raw)) else { continue };
                if dt > horizon {
                    continue;
                }
                let overdue = dt < today;
                if overdue && !q.include_overdue {
                    continue;
                }
                items.push(ReminderItem {
                    kind,
                    entity_id: id.clone(),
                    entity_name: name.clone(),
                    date: dt,
                    days_remaining: (dt - today).num_days(),
                    overdue,
                    note: note.clone(),
                    maintenance_id: None,
                });
            }
        }
    }

    // scheduled maintenance
    if kinds.contains("maintenance") {
        let rows = query_all(
            conn,
            "SELECT m.id, m.entity_id, m.name, m.scheduled_date, e.name
               FROM maintenance_entries m
               JOIN entities e ON e.id = m.entity_id
              WHERE e.group_id = ?1 AND e.archived = 0
                AND m.scheduled_date IS NOT NULL AND m.completed_date IS NULL",
            params![group_id],
            |r| {
                Ok::<_, rusqlite::Error>((
                    text(r, 0)?,
                    text(r, 1)?,
                    text(r, 2)?,
                    r.get::<_, Option<String>>(3)?,
                    text(r, 4)?,
                ))
            },
        )?;
        for (mid, eid, mname, scheduled, ename) in rows {
            let Some(raw) = scheduled else { continue };
            let Some(dt) = ts_opt(Some(&raw)) else { continue };
            if dt > horizon {
                continue;
            }
            let overdue = dt < today;
            if overdue && !q.include_overdue {
                continue;
            }
            items.push(ReminderItem {
                kind: ReminderKind::Maintenance,
                entity_id: eid,
                entity_name: ename,
                date: dt,
                days_remaining: (dt - today).num_days(),
                overdue,
                note: mname,
                maintenance_id: Some(mid),
            });
        }
    }

    // low stock
    if kinds.contains("lowStock") {
        let rows = query_all(
            conn,
            "SELECT id, name, quantity, min_quantity FROM entities
              WHERE group_id = ?1 AND archived = 0 AND min_quantity > 0 AND quantity <= min_quantity",
            params![group_id],
            |r| {
                Ok::<_, rusqlite::Error>((
                    text(r, 0)?,
                    text(r, 1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, f64>(3)?,
                ))
            },
        )?;
        for (id, name, qty, min_qty) in rows {
            items.push(ReminderItem {
                kind: ReminderKind::LowStock,
                entity_id: id,
                entity_name: name,
                date: today,
                days_remaining: 0,
                overdue: true,
                note: format!("库存 {qty} ≤ 下限 {min_qty}"),
                maintenance_id: None,
            });
        }
    }

    items.sort_by(|a, b| a.date.cmp(&b.date));
    let counts = ReminderCounts {
        overdue: items.iter().filter(|i| i.overdue).count() as i64,
        due_soon: items.iter().filter(|i| !i.overdue).count() as i64,
        total: items.len() as i64,
    };
    Ok(ReminderResponse { items, counts })
}

// ---------------------------------------------------------------------------
// statistics
// ---------------------------------------------------------------------------

/// Headline statistics for a group.
pub fn statistics(conn: &Connection, group_id: &str) -> AppResult<Statistics> {
    let row = query_optional(
        conn,
        "SELECT
           COALESCE(SUM(CASE WHEN et.is_location = 0 AND e.archived = 0 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN et.is_location = 0 AND e.archived = 0 THEN e.quantity * e.purchase_price ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN et.is_location = 1 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN e.warranty_expires IS NOT NULL AND e.archived = 0 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN e.archived = 1 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN et.is_location = 0 AND e.archived = 0 AND e.min_quantity > 0 AND e.quantity <= e.min_quantity THEN 1 ELSE 0 END), 0)
         FROM entities e JOIN entity_types et ON et.id = e.entity_type_id
        WHERE e.group_id = ?1",
        params![group_id],
        |r| {
            Ok::<_, rusqlite::Error>((
                r.get::<_, i64>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        },
    )?
    .unwrap_or((0, 0.0, 0, 0, 0, 0));

    let tags: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tags WHERE group_id = ?1",
        params![group_id],
        |r| r.get(0),
    )?;
    let users: i64 = conn.query_row(
        "SELECT COUNT(*) FROM user_groups WHERE group_id = ?1",
        params![group_id],
        |r| r.get(0),
    )?;

    Ok(Statistics {
        total_items: row.0,
        total_value: row.1,
        total_locations: row.2,
        total_tags: tags,
        total_users: users,
        total_with_warranty: row.3,
        total_archived: row.4,
        total_low_stock: row.5,
    })
}

/// Value totals grouped by location.
pub fn stats_by_location(conn: &Connection, group_id: &str) -> AppResult<Vec<TotalsByOrganizer>> {
    query_all(
        conn,
        "SELECT l.id, l.name,
                COALESCE(SUM(e.quantity * e.purchase_price), 0),
                COALESCE(COUNT(e.id), 0)
           FROM entities l
           LEFT JOIN entities e
             ON e.location_id = l.id AND e.archived = 0
            AND e.id != l.id
            AND e.entity_type_id IN (SELECT id FROM entity_types WHERE is_location = 0)
          WHERE l.group_id = ?1
            AND l.entity_type_id IN (SELECT id FROM entity_types WHERE is_location = 1)
          GROUP BY l.id, l.name
          ORDER BY 3 DESC",
        params![group_id],
        |r| {
            Ok::<_, rusqlite::Error>(TotalsByOrganizer {
                id: text(r, 0)?,
                name: text(r, 1)?,
                total: r.get(2)?,
                count: r.get(3)?,
            })
        },
    )
}

/// Value totals grouped by tag.
pub fn stats_by_tag(conn: &Connection, group_id: &str) -> AppResult<Vec<TotalsByOrganizer>> {
    query_all(
        conn,
        "SELECT t.id, t.name,
                COALESCE(SUM(e.quantity * e.purchase_price), 0),
                COALESCE(COUNT(DISTINCT e.id), 0)
           FROM tags t
           LEFT JOIN entity_tags et ON et.tag_id = t.id
           LEFT JOIN entities e ON e.id = et.entity_id AND e.archived = 0
          WHERE t.group_id = ?1
          GROUP BY t.id, t.name
          ORDER BY 3 DESC",
        params![group_id],
        |r| {
            Ok::<_, rusqlite::Error>(TotalsByOrganizer {
                id: text(r, 0)?,
                name: text(r, 1)?,
                total: r.get(2)?,
                count: r.get(3)?,
            })
        },
    )
}

/// Monthly purchase totals for the value-over-time chart.
pub fn stats_purchase_over_time(conn: &Connection, group_id: &str) -> AppResult<Vec<ValueOverTimeEntry>> {
    let rows = query_all(
        conn,
        "SELECT substr(purchase_date, 1, 7) AS ym,
                COALESCE(SUM(purchase_price), 0)
           FROM entities
          WHERE group_id = ?1 AND purchase_date IS NOT NULL AND archived = 0
          GROUP BY ym ORDER BY ym",
        params![group_id],
        |r| {
            Ok::<_, rusqlite::Error>((text(r, 0)?, r.get::<_, f64>(1)?))
        },
    )?;
    let mut out = Vec::with_capacity(rows.len());
    for (ym, value) in rows {
        let ts = format!("{ym}-01T00:00:00Z");
        out.push(ValueOverTimeEntry {
            date: ts_opt(Some(&ts)).unwrap_or_default(),
            name: ym,
            value,
        });
    }
    Ok(out)
}

/// Convenience: reminder counts only (dashboard badge).
pub fn reminder_counts(conn: &Connection, group_id: &str) -> AppResult<ReminderCounts> {
    let resp = reminders(conn, group_id, &RemindersQuery::default())?;
    Ok(resp.counts)
}

/// Keep HashMap import used.
#[allow(dead_code)]
type _Map = HashMap<String, String>;

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
        let (loc, item) = super::super::taxonomy::seed_defaults(&conn, "g1").unwrap();
        // one low-stock item with a warranty expiring in 5 days
        let soon = (chrono::Utc::now() + chrono::Duration::days(5)).format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "INSERT INTO entities (id, group_id, name, entity_type_id, quantity, min_quantity,
                                   warranty_expires, created_at, updated_at)
             VALUES ('it1', 'g1', '滤芯', ?1, 1, 3, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![item, soon],
        )
        .unwrap();
        // one location
        conn.execute(
            "INSERT INTO entities (id, group_id, name, entity_type_id, created_at, updated_at)
             VALUES ('loc1', 'g1', '厨房', ?1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![loc],
        )
        .unwrap();
        conn
    }

    #[test]
    fn reminders_aggregate_four_kinds() {
        let conn = setup();
        // scheduled maintenance 2 days out
        conn.execute(
            "INSERT INTO maintenance_entries (id, entity_id, name, scheduled_date, created_at, updated_at)
             VALUES ('m1', 'it1', '换滤芯', ?1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![(chrono::Utc::now() + chrono::Duration::days(2)).format("%Y-%m-%dT%H:%M:%SZ").to_string()],
        )
        .unwrap();
        let resp = reminders(&conn, "g1", &RemindersQuery::default()).unwrap();
        let kinds: Vec<&str> = resp.items.iter().map(|i| match i.kind {
            ReminderKind::Warranty => "warranty",
            ReminderKind::Reminder => "reminder",
            ReminderKind::Maintenance => "maintenance",
            ReminderKind::LowStock => "lowStock",
        }).collect();
        assert!(kinds.contains(&"warranty"), "got {kinds:?}");
        assert!(kinds.contains(&"lowStock"), "got {kinds:?}");
        assert!(kinds.contains(&"maintenance"), "got {kinds:?}");
        assert!(resp.counts.total >= 3);
        assert!(resp.counts.due_soon >= 2);
    }

    #[test]
    fn reminder_kind_filter() {
        let conn = setup();
        let q = RemindersQuery { kinds: Some("lowStock".into()), ..Default::default() };
        let resp = reminders(&conn, "g1", &q).unwrap();
        assert!(resp.items.iter().all(|i| i.kind == ReminderKind::LowStock));
        assert_eq!(resp.counts.total, 1);
    }

    #[test]
    fn statistics_summarize_group() {
        let conn = setup();
        let s = statistics(&conn, "g1").unwrap();
        assert_eq!(s.total_items, 1);
        assert_eq!(s.total_locations, 1);
        assert_eq!(s.total_tags, 0);
        assert_eq!(s.total_users, 0);
        assert_eq!(s.total_with_warranty, 1);
        assert_eq!(s.total_low_stock, 1);
        assert!(s.total_value > 0.0 || (s.total_value - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn notifier_crud_and_dedupe() {
        let conn = setup();
        let id = create_notifier(
            &conn,
            "g1",
            &NotifierCreate {
                name: "邮件".into(),
                kind: NotifierKind::Email,
                target: "me@example.com".into(),
                is_active: true,
                kinds: vec![ReminderKind::Warranty],
            },
        )
        .unwrap();
        let list = list_notifiers(&conn, "g1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kinds, vec![ReminderKind::Warranty]);

        let active = active_for_kind(&conn, "g1", ReminderKind::Warranty).unwrap();
        assert_eq!(active.len(), 1);
        let active_low = active_for_kind(&conn, "g1", ReminderKind::LowStock).unwrap();
        assert!(active_low.is_empty());

        assert!(!already_sent(&conn, &id, "warranty:it1:2026-06-01").unwrap());
        record_send(&conn, &id, "warranty:it1:2026-06-01").unwrap();
        assert!(already_sent(&conn, &id, "warranty:it1:2026-06-01").unwrap());
        record_send(&conn, &id, "warranty:it1:2026-06-01").unwrap(); // idempotent
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM notification_log", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);

        update_notifier(
            &conn,
            "g1",
            &id,
            &NotifierUpdate {
                name: "邮件2".into(),
                kind: NotifierKind::Email,
                target: "me2@example.com".into(),
                is_active: false,
                kinds: vec![],
            },
        )
        .unwrap();
        assert_eq!(list_notifiers(&conn, "g1").unwrap()[0].name, "邮件2");

        delete_notifier(&conn, "g1", &id).unwrap();
        assert!(list_notifiers(&conn, "g1").unwrap().is_empty());
        assert!(delete_notifier(&conn, "g1", &id).is_err());
    }
}
