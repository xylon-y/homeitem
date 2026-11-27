//! Entities (items + locations share one table, mirroring upstream).
//!
//! Location resolution follows upstream HomeBox #1688: an entity's own
//! `location_id` wins, otherwise the nearest location ancestor along the
//! `parent_id` chain.

use std::collections::{HashMap, HashSet};

use common::dto::{
    EntityCreate, EntityFieldData, EntityOut, EntityPath, EntitySummary, EntityUpdate, EntityTypeOut,
    ItemAttachment, MaintenanceEntry, Page, TagSummary, TreeItem,
};
use common::query::EntitiesQuery;
use rusqlite::{params, Connection, OptionalExtension, ToSql};

use super::{bool_sql, from_bool, query_all, query_optional, text, ts_opt, ts_or_epoch};
use crate::error::{AppError, AppResult};

// ---------------------------------------------------------------------------
// row projections
// ---------------------------------------------------------------------------

/// Shared columns (list + detail prefix).
const COLS: &str = "
  e.id, e.name, e.description, e.quantity, e.min_quantity, e.asset_id, e.archived,
  e.insured, e.entity_type_id, e.parent_id, e.location_id, e.warranty_expires,
  e.reminder_date, e.sold_date, e.sold_price, e.purchase_price, e.created_at, e.updated_at,
  et.id, et.name, et.description, et.icon, et.is_location,
  p.id, p.name, l.id, l.name
";

const FROM: &str = "
  FROM entities e
  JOIN entity_types et ON et.id = e.entity_type_id
  LEFT JOIN entities p ON p.id = e.parent_id
  LEFT JOIN entities l ON l.id = e.location_id
";

/// Extra detail-only columns appended after `COLS`.
const DETAIL_EXTRA: &str = ", e.serial_number, e.model_number, e.manufacturer, e.notes,
  e.lifetime_warranty, e.warranty_details, e.purchase_date, e.purchase_from,
  e.sold_to, e.sold_notes, e.reminder_note, e.sync_child_entity_locations
";

/// Column indices of `COLS`.
mod c {
    pub const ID: usize = 0;
    pub const NAME: usize = 1;
    pub const DESC: usize = 2;
    pub const QTY: usize = 3;
    pub const MIN_QTY: usize = 4;
    pub const ASSET: usize = 5;
    pub const ARCHIVED: usize = 6;
    pub const INSURED: usize = 7;
    pub const TYPE_ID: usize = 8;
    pub const PARENT_ID: usize = 9;
    pub const LOC_ID: usize = 10;
    pub const WARRANTY: usize = 11;
    pub const REMINDER: usize = 12;
    pub const SOLD_DATE: usize = 13;
    pub const SOLD_PRICE: usize = 14;
    pub const PRICE: usize = 15;
    pub const CREATED: usize = 16;
    pub const UPDATED: usize = 17;
    pub const ET_ID: usize = 18;
    pub const ET_NAME: usize = 19;
    pub const ET_DESC: usize = 20;
    pub const ET_ICON: usize = 21;
    pub const ET_IS_LOC: usize = 22;
    pub const P_ID: usize = 23;
    pub const P_NAME: usize = 24;
    pub const L_ID: usize = 25;
    pub const L_NAME: usize = 26;
    // detail extras start at 27
    pub const SERIAL: usize = 27;
    pub const MODEL: usize = 28;
    pub const MFR: usize = 29;
    pub const NOTES: usize = 30;
    pub const LIFETIME_W: usize = 31;
    pub const W_DETAILS: usize = 32;
    pub const PURCHASE_DATE: usize = 33;
    pub const PURCHASE_FROM: usize = 34;
    pub const SOLD_TO: usize = 35;
    pub const SOLD_NOTES: usize = 36;
    pub const REMINDER_NOTE: usize = 37;
    pub const SYNC_CHILD: usize = 38;
}

/// Lightweight row shared by list and detail.
struct Row0 {
    id: String,
    name: String,
    description: String,
    quantity: f64,
    min_quantity: f64,
    asset_id: i64,
    archived: bool,
    insured: bool,
    parent_id: Option<String>,
    parent_name: Option<String>,
    location_id: Option<String>,
    location_name: Option<String>,
    warranty_expires: Option<String>,
    reminder_date: Option<String>,
    sold_date: Option<String>,
    sold_price: f64,
    purchase_price: f64,
    created_at: String,
    updated_at: String,
    et: EntityTypeOut,
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row0> {
    Ok(Row0 {
        id: text(row, c::ID)?,
        name: text(row, c::NAME)?,
        description: text(row, c::DESC)?,
        quantity: row.get(c::QTY)?,
        min_quantity: row.get(c::MIN_QTY)?,
        asset_id: row.get(c::ASSET)?,
        archived: from_bool(row.get(c::ARCHIVED)?),
        insured: from_bool(row.get(c::INSURED)?),
        parent_id: row.get(c::PARENT_ID)?,
        parent_name: row.get(c::P_NAME)?,
        location_id: row.get(c::LOC_ID)?,
        location_name: row.get(c::L_NAME)?,
        warranty_expires: row.get(c::WARRANTY)?,
        reminder_date: row.get(c::REMINDER)?,
        sold_date: row.get(c::SOLD_DATE)?,
        sold_price: row.get(c::SOLD_PRICE)?,
        purchase_price: row.get(c::PRICE)?,
        created_at: text(row, c::CREATED)?,
        updated_at: text(row, c::UPDATED)?,
        et: EntityTypeOut {
            id: text(row, c::ET_ID)?,
            name: text(row, c::ET_NAME)?,
            description: text(row, c::ET_DESC)?,
            icon: text(row, c::ET_ICON)?,
            is_location: from_bool(row.get(c::ET_IS_LOC)?),
            created_at: ts_or_epoch(None),
            updated_at: ts_or_epoch(None),
        },
    })
}

/// Walk up `parent_id` from `start` and return the nearest ancestor that is a
/// location (entity type with `is_location = 1`). Cycle-guarded.
fn resolve_location_ancestor(
    conn: &Connection,
    group_id: &str,
    start: &str,
) -> AppResult<Option<(String, String)>> {
    let mut cur = start.to_string();
    let mut seen = HashSet::new();
    for _ in 0..128 {
        if !seen.insert(cur.clone()) {
            return Ok(None); // cycle
        }
        let parent: Option<String> = conn
            .query_row(
                "SELECT parent_id FROM entities WHERE id = ?1 AND group_id = ?2",
                params![cur, group_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let Some(p) = parent else { return Ok(None) };
        let loc: Option<(String, String)> = conn
            .query_row(
                "SELECT e.id, e.name FROM entities e
                   JOIN entity_types et ON et.id = e.entity_type_id
                  WHERE e.id = ?1 AND e.group_id = ?2 AND et.is_location = 1",
                params![p, group_id],
                |r| Ok::<_, rusqlite::Error>((text(r, 0)?, text(r, 1)?)),
            )
            .optional()?;
        if loc.is_some() {
            return Ok(loc);
        }
        cur = p;
    }
    Ok(None)
}

fn stub_summary(id: &str, name: &str) -> EntitySummary {
    EntitySummary {
        id: id.to_string(),
        name: name.to_string(),
        description: String::new(),
        quantity: 0.0,
        min_quantity: 0.0,
        asset_id: 0,
        archived: false,
        insured: false,
        entity_type: None,
        parent: None,
        location: None,
        location_id: None,
        tags: Vec::new(),
        thumbnail_id: None,
        purchase_price: 0.0,
        total_price: 0.0,
        warranty_expires: None,
        reminder_date: None,
        sold_date: None,
        sold_price: 0.0,
        item_count: 0,
        created_at: ts_or_epoch(None),
        updated_at: ts_or_epoch(None),
    }
}

fn summary_of(r: &Row0, tags: Vec<TagSummary>, thumbnail: Option<String>, item_count: i64) -> EntitySummary {
    EntitySummary {
        id: r.id.clone(),
        name: r.name.clone(),
        description: r.description.clone(),
        quantity: r.quantity,
        min_quantity: r.min_quantity,
        asset_id: r.asset_id,
        archived: r.archived,
        insured: r.insured,
        entity_type: Some(r.et.clone()),
        parent: r
            .parent_id
            .as_ref()
            .map(|id| Box::new(stub_summary(id, r.parent_name.as_deref().unwrap_or_default()))),
        location: r
            .location_id
            .as_ref()
            .map(|id| Box::new(stub_summary(id, r.location_name.as_deref().unwrap_or_default()))),
        location_id: r.location_id.clone(),
        tags,
        thumbnail_id: thumbnail,
        purchase_price: r.purchase_price,
        total_price: r.purchase_price * r.quantity,
        warranty_expires: ts_opt(r.warranty_expires.as_deref()),
        reminder_date: ts_opt(r.reminder_date.as_deref()),
        sold_date: ts_opt(r.sold_date.as_deref()),
        sold_price: r.sold_price,
        item_count,
        created_at: ts_or_epoch(Some(&r.created_at)),
        updated_at: ts_or_epoch(Some(&r.updated_at)),
    }
}

// ---------------------------------------------------------------------------
// filtering
// ---------------------------------------------------------------------------

/// Build `WHERE` fragment (including leading AND-conditions) + bound args.
fn build_where(group_id: &str, q: &EntitiesQuery) -> (String, Vec<String>) {
    let mut sql = String::from(" WHERE e.group_id = ?1");
    let mut args: Vec<String> = vec![group_id.to_string()];

    if !q.include_archived {
        sql.push_str(" AND e.archived = 0");
    }
    if let Some(is_loc) = q.is_location {
        sql.push_str(" AND et.is_location = ?");
        args.push(bool_sql(is_loc).to_string());
    }
    if !q.q.trim().is_empty() {
        sql.push_str(
            " AND (e.name LIKE ? ESCAPE '\\' OR e.description LIKE ? ESCAPE '\\' OR e.serial_number LIKE ? ESCAPE '\\'
                   OR e.model_number LIKE ? ESCAPE '\\' OR e.manufacturer LIKE ? ESCAPE '\\' OR e.notes LIKE ? ESCAPE '\\'
                   OR CAST(e.asset_id AS TEXT) LIKE ? ESCAPE '\\')",
        );
        let escaped = q
            .q
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pat = format!("%{}%", escaped);
        for _ in 0..7 {
            args.push(pat.clone());
        }
    }
    let parent_ids = EntitiesQuery::id_list(&q.parent_ids);
    if !parent_ids.is_empty() {
        let ph = vec!["?"; parent_ids.len()].join(",");
        sql.push_str(&format!(" AND e.parent_id IN ({ph})"));
        args.extend(parent_ids);
    }
    let type_ids = EntitiesQuery::id_list(&q.entity_type_ids);
    if !type_ids.is_empty() {
        let ph = vec!["?"; type_ids.len()].join(",");
        sql.push_str(&format!(" AND e.entity_type_id IN ({ph})"));
        args.extend(type_ids);
    }

    let tag_ids = EntitiesQuery::id_list(&q.tags);
    if !tag_ids.is_empty() {
        let ph = vec!["?"; tag_ids.len()].join(",");
        if q.match_all_tags {
            sql.push_str(&format!(
                " AND (SELECT COUNT(DISTINCT et2.tag_id) FROM entity_tags et2
                        WHERE et2.entity_id = e.id AND et2.tag_id IN ({ph})) = {n}",
                n = tag_ids.len()
            ));
        } else if q.negate_tags {
            sql.push_str(&format!(
                " AND NOT EXISTS (SELECT 1 FROM entity_tags et3
                                   WHERE et3.entity_id = e.id AND et3.tag_id IN ({ph}))"
            ));
        } else {
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM entity_tags et4
                               WHERE et4.entity_id = e.id AND et4.tag_id IN ({ph}))"
            ));
        }
        args.extend(tag_ids.iter().cloned());
    }
    if q.only_with_photo {
        sql.push_str(" AND EXISTS (SELECT 1 FROM attachments a WHERE a.entity_id = e.id AND a.primary_photo = 1)");
    }
    if q.only_without_photo {
        sql.push_str(" AND NOT EXISTS (SELECT 1 FROM attachments a WHERE a.entity_id = e.id AND a.primary_photo = 1)");
    }
    if q.low_stock {
        sql.push_str(" AND e.min_quantity > 0 AND e.quantity <= e.min_quantity");
    }
    (sql, args)
}

/// Run a query with `?1..n` positional args given as strings.
fn query_str<T>(
    conn: &Connection,
    sql: &str,
    args: &[String],
    map: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> AppResult<Vec<T>> {
    let refs: Vec<&dyn ToSql> = args.iter().map(|s| s as &dyn ToSql).collect();
    query_all(conn, sql, &refs, map)
}

fn query_one_str<T>(
    conn: &Connection,
    sql: &str,
    args: &[String],
    map: impl FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> AppResult<Option<T>> {
    let refs: Vec<&dyn ToSql> = args.iter().map(|s| s as &dyn ToSql).collect();
    query_optional(conn, sql, &refs, map)
}

// ---------------------------------------------------------------------------
// enrichment
// ---------------------------------------------------------------------------

/// Primary thumbnail attachment id per entity id.
fn thumbnails(conn: &Connection, ids: &[String]) -> AppResult<HashMap<String, String>> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let ph = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT entity_id, id FROM attachments
          WHERE primary_photo = 1 AND entity_id IN ({ph})"
    );
    let refs: Vec<&dyn ToSql> = ids.iter().map(|s| s as &dyn ToSql).collect();
    for (eid, aid) in query_all(conn, &sql, &refs, |r| {
        Ok::<_, rusqlite::Error>((text(r, 0)?, text(r, 1)?))
    })? {
        out.insert(eid, aid);
    }
    Ok(out)
}

/// Per-location item counts (non-location entities whose resolved location matches).
fn item_counts(conn: &Connection, group_id: &str) -> AppResult<HashMap<String, i64>> {
    // Resolve locations in Rust: load minimal graph, walk parent chains.
    let rows = query_all(
        conn,
        "SELECT e.id, e.parent_id, e.location_id, et.is_location, e.archived
           FROM entities e JOIN entity_types et ON et.id = e.entity_type_id
          WHERE e.group_id = ?1",
        &[&group_id],
        |r| {
            Ok::<_, rusqlite::Error>((
                text(r, 0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                from_bool(r.get(3)?),
                from_bool(r.get(4)?),
            ))
        },
    )?;

    let mut parent_of: HashMap<String, Option<String>> = HashMap::new();
    let mut own_loc: HashMap<String, Option<String>> = HashMap::new();
    let mut is_loc: HashSet<String> = HashSet::new();
    let mut archived: HashSet<String> = HashSet::new();
    for (id, p, l, loc, arch) in rows {
        parent_of.insert(id.clone(), p);
        own_loc.insert(id.clone(), l);
        if loc {
            is_loc.insert(id.clone());
        }
        if arch {
            archived.insert(id);
        }
    }

    let resolve = |start: &str| -> Option<String> {
        let mut cur = start.to_string();
        let mut hops = 0;
        loop {
            if hops > 64 {
                return None; // cycle guard
            }
            if let Some(l) = own_loc.get(&cur) {
                if let Some(l) = l {
                    return Some(l.clone());
                }
            }
            if is_loc.contains(&cur) && cur != start {
                return Some(cur.clone());
            }
            match parent_of.get(&cur) {
                Some(Some(p)) => {
                    cur = p.clone();
                    hops += 1;
                }
                _ => return None,
            }
        }
    };

    let mut counts: HashMap<String, i64> = HashMap::new();
    for (id, _, _, loc, arch) in query_all(
        conn,
        "SELECT e.id, 0, 0, et.is_location, e.archived
           FROM entities e JOIN entity_types et ON et.id = e.entity_type_id
          WHERE e.group_id = ?1",
        &[&group_id],
        |r| {
            Ok::<_, rusqlite::Error>((
                text(r, 0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                from_bool(r.get(3)?),
                from_bool(r.get(4)?),
            ))
        },
    )? {
        let _ = &archived;
        if loc || arch {
            continue;
        }
        if let Some(l) = resolve(&id) {
            *counts.entry(l).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

// ---------------------------------------------------------------------------
// public API
// ---------------------------------------------------------------------------

/// Paginated, filtered entity list.
pub fn list(
    conn: &Connection,
    group_id: &str,
    q: &EntitiesQuery,
) -> AppResult<Page<EntitySummary>> {
    let (where_sql, args) = build_where(group_id, q);

    let count_sql = format!("SELECT COUNT(*) FROM entities e JOIN entity_types et ON et.id = e.entity_type_id {where_sql}");
    let refs: Vec<&dyn ToSql> = args.iter().map(|s| s as &dyn ToSql).collect();
    let total: i64 = conn.query_row(&count_sql, refs.as_slice(), |r| r.get(0))?;

    let order = match q.order_column() {
        "created_at" => "e.created_at",
        "updated_at" => "e.updated_at",
        "purchase_price" => "e.purchase_price",
        "warranty_expires" => "e.warranty_expires",
        "asset_id" => "e.asset_id",
        "quantity" => "e.quantity",
        _ => "e.name COLLATE NOCASE",
    };
    let mut data_args = args.clone();
    data_args.push(q.page_size().to_string());
    data_args.push(((q.page() - 1) * q.page_size()).to_string());
    let data_sql = format!(
        "SELECT {COLS} {FROM}{where_sql} ORDER BY {order} {} LIMIT ?{} OFFSET ?{}",
        q.order_dir(),
        args.len() + 1,
        args.len() + 2
    );
    let rows = query_str(conn, &data_sql, &data_args, map_row)?;

    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let tags = super::taxonomy::tags_for_entities(conn, &ids)?;
    let thumbs = thumbnails(conn, &ids)?;
    let counts = if q.is_location == Some(true) {
        item_counts(conn, group_id)?
    } else {
        HashMap::new()
    };

    let items = rows
        .iter()
        .map(|r| {
            summary_of(
                r,
                tags.get(&r.id).cloned().unwrap_or_default(),
                thumbs.get(&r.id).cloned(),
                counts.get(&r.id).copied().unwrap_or(0),
            )
        })
        .collect();
    Ok(Page::new(items, q.page(), q.page_size(), total))
}

/// Full detail including fields, attachments and maintenance count.
pub fn get(conn: &Connection, group_id: &str, id: &str) -> AppResult<EntityOut> {
    let sql = format!("SELECT {COLS}{DETAIL_EXTRA} {FROM} WHERE e.group_id = ?1 AND e.id = ?2");
    let r = query_one_str(conn, &sql, &[group_id.to_string(), id.to_string()], map_row)?
        .ok_or_else(|| AppError::NotFound("物品不存在".into()))?;

    let detail = query_one_str(
        conn,
        &sql,
        &[group_id.to_string(), id.to_string()],
        |row| {
            Ok::<_, rusqlite::Error>(DetailCols {
                serial: text(row, c::SERIAL)?,
                model: text(row, c::MODEL)?,
                manufacturer: text(row, c::MFR)?,
                notes: text(row, c::NOTES)?,
                lifetime_warranty: from_bool(row.get(c::LIFETIME_W)?),
                warranty_details: text(row, c::W_DETAILS)?,
                purchase_date: row.get::<_, Option<String>>(c::PURCHASE_DATE)?,
                purchase_from: text(row, c::PURCHASE_FROM)?,
                sold_to: text(row, c::SOLD_TO)?,
                sold_notes: text(row, c::SOLD_NOTES)?,
                reminder_note: text(row, c::REMINDER_NOTE)?,
                sync_child: from_bool(row.get(c::SYNC_CHILD)?),
            })
        },
    )?
    .expect("row just fetched");

    let tags = super::taxonomy::tags_for_entity(conn, id)?;
    let thumbs = thumbnails(conn, &[id.to_string()])?;
    let fields = list_fields(conn, id)?;
    let attachments = super::attachments::list(conn, id)?;
    let maintenance_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM maintenance_entries WHERE entity_id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    let child_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entities WHERE parent_id = ?1",
        params![id],
        |row| row.get(0),
    )?;

    let mut summary = summary_of(&r, tags, thumbs.get(id).cloned(), child_count);
    // Upstream behavior: an item inherits the nearest location ancestor when
    // it has no explicit location_id. Keep detail view consistent with the
    // location tree / statistics (which already resolve through parent_id).
    if summary.location_id.is_none() {
        if let Some((loc_id, loc_name)) = resolve_location_ancestor(conn, group_id, id)? {
            summary.location_id = Some(loc_id.clone());
            summary.location = Some(Box::new(stub_summary(&loc_id, &loc_name)));
        }
    }
    Ok(EntityOut {
        summary,
        serial_number: detail.serial,
        model_number: detail.model,
        manufacturer: detail.manufacturer,
        notes: detail.notes,
        lifetime_warranty: detail.lifetime_warranty,
        warranty_details: detail.warranty_details,
        purchase_date: ts_opt(detail.purchase_date.as_deref()),
        purchase_from: detail.purchase_from,
        sold_to: detail.sold_to,
        sold_notes: detail.sold_notes,
        reminder_note: detail.reminder_note,
        sync_child_entity_locations: detail.sync_child,
        fields,
        attachments,
        maintenance_count,
    })
}

struct DetailCols {
    serial: String,
    model: String,
    manufacturer: String,
    notes: String,
    lifetime_warranty: bool,
    warranty_details: String,
    purchase_date: Option<String>,
    purchase_from: String,
    sold_to: String,
    sold_notes: String,
    reminder_note: String,
    sync_child: bool,
}

/// Custom fields of an entity.
pub fn list_fields(conn: &Connection, entity_id: &str) -> AppResult<Vec<EntityFieldData>> {
    query_all(
        conn,
        "SELECT id, name, type, text_value, number_value, boolean_value
           FROM entity_fields WHERE entity_id = ?1 ORDER BY name",
        params![entity_id],
        |row| {
            Ok::<_, rusqlite::Error>(EntityFieldData {
                id: text(row, 0)?,
                name: text(row, 1)?,
                kind: text(row, 2)?,
                text_value: text(row, 3)?,
                number_value: row.get(4)?,
                boolean_value: from_bool(row.get(5)?),
            })
        },
    )
}

/// Replace all custom fields of an entity.
fn set_fields(conn: &Connection, entity_id: &str, fields: &[EntityFieldData]) -> AppResult<()> {
    conn.execute("DELETE FROM entity_fields WHERE entity_id = ?1", params![entity_id])?;
    for f in fields {
        conn.execute(
            "INSERT INTO entity_fields (id, entity_id, name, type, text_value, number_value, boolean_value)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                f.id,
                entity_id,
                f.name,
                f.kind,
                f.text_value,
                f.number_value,
                bool_sql(f.boolean_value)
            ],
        )?;
    }
    Ok(())
}

/// Distinct custom field names in a group.
pub fn field_names(conn: &Connection, group_id: &str) -> AppResult<Vec<String>> {
    query_all(
        conn,
        "SELECT DISTINCT ef.name FROM entity_fields ef
           JOIN entities e ON e.id = ef.entity_id
          WHERE e.group_id = ?1 ORDER BY ef.name",
        params![group_id],
        |row| text(row, 0),
    )
}

/// Distinct values of one custom field.
pub fn field_values(conn: &Connection, group_id: &str, name: &str) -> AppResult<Vec<String>> {
    query_all(
        conn,
        "SELECT DISTINCT ef.text_value FROM entity_fields ef
           JOIN entities e ON e.id = ef.entity_id
          WHERE e.group_id = ?1 AND ef.name = ?2 ORDER BY ef.text_value",
        params![group_id, name],
        |row| text(row, 0),
    )
}

/// Create an entity; caller must have validated references.
pub fn create(
    conn: &Connection,
    group_id: &str,
    id: &str,
    req: &EntityCreate,
) -> AppResult<()> {
    let now = super::now();
    conn.execute(
        "INSERT INTO entities (id, group_id, name, description, quantity, min_quantity,
                               parent_id, location_id, entity_type_id, serial_number,
                               model_number, manufacturer, notes, purchase_price,
                               purchase_date, purchase_from, warranty_expires,
                               created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18)",
        params![
            id,
            group_id,
            req.name,
            req.description,
            req.quantity,
            req.min_quantity,
            req.parent_id,
            req.location_id,
            req.entity_type_id,
            req.serial_number,
            req.model_number,
            req.manufacturer,
            req.notes,
            req.purchase_price,
            super::ts_to_sql(req.purchase_date),
            req.purchase_from,
            super::ts_to_sql(req.warranty_expires),
            now
        ],
    )?;
    super::taxonomy::set_entity_tags(conn, id, &req.tag_ids)?;
    Ok(())
}

/// Full update of an entity.
pub fn update(
    conn: &Connection,
    group_id: &str,
    id: &str,
    req: &EntityUpdate,
) -> AppResult<()> {
    let now = super::now();
    let n = conn.execute(
        "UPDATE entities SET name = ?1, description = ?2, quantity = ?3, min_quantity = ?4,
                            parent_id = ?5, location_id = ?6, entity_type_id = ?7,
                            archived = ?8, insured = ?9, asset_id = ?10,
                            serial_number = ?11, model_number = ?12, manufacturer = ?13,
                            notes = ?14, lifetime_warranty = ?15, warranty_expires = ?16,
                            warranty_details = ?17, purchase_date = ?18, purchase_from = ?19,
                            purchase_price = ?20, sold_date = ?21, sold_to = ?22,
                            sold_price = ?23, sold_notes = ?24, reminder_date = ?25,
                            reminder_note = ?26, sync_child_entity_locations = ?27,
                            updated_at = ?28
          WHERE id = ?29 AND group_id = ?30",
        params![
            req.name,
            req.description,
            req.quantity,
            req.min_quantity,
            req.parent_id,
            req.location_id,
            req.entity_type_id,
            bool_sql(req.archived),
            bool_sql(req.insured),
            req.asset_id,
            req.serial_number,
            req.model_number,
            req.manufacturer,
            req.notes,
            bool_sql(req.lifetime_warranty),
            ts_to_sql(req.warranty_expires),
            req.warranty_details,
            ts_to_sql(req.purchase_date),
            req.purchase_from,
            req.purchase_price,
            ts_to_sql(req.sold_date),
            req.sold_to,
            req.sold_price,
            req.sold_notes,
            ts_to_sql(req.reminder_date),
            req.reminder_note,
            bool_sql(req.sync_child_entity_locations),
            now,
            id,
            group_id
        ],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("物品不存在".into()));
    }
    super::taxonomy::set_entity_tags(conn, id, &req.tag_ids)?;
    set_fields(conn, id, &req.fields)?;
    Ok(())
}

use super::ts_to_sql;

/// Partial update.
pub fn patch(
    conn: &Connection,
    group_id: &str,
    id: &str,
    req: &common::dto::EntityPatch,
) -> AppResult<()> {
    let now = super::now();
    if let Some(type_id) = &req.entity_type_id {
        conn.execute(
            "UPDATE entities SET entity_type_id = ?1, updated_at = ?2 WHERE id = ?3 AND group_id = ?4",
            params![type_id, now, id, group_id],
        )?;
    }
    if let Some(parent) = &req.parent_id {
        // Reject self-reference and cycles: walking `parent_id` upward must
        // never return to `id`, otherwise the entity silently vanishes from
        // the location tree and breadcrumbs truncate.
        if let Some(p) = parent {
            if p == id {
                return Err(AppError::Validation("上级不能是自身".into()));
            }
            let mut cur = p.clone();
            let mut hops = 0;
            while hops < 128 {
                let next: Option<String> = conn
                    .query_row(
                        "SELECT parent_id FROM entities WHERE id = ?1 AND group_id = ?2",
                        params![cur, group_id],
                        |r| r.get(0),
                    )
                    .optional()
                    .unwrap_or(None)
                    .flatten();
                match next {
                    Some(n) if n == id => {
                        return Err(AppError::Validation("上级层级不能形成循环".into()))
                    }
                    Some(n) => cur = n,
                    None => break,
                }
                hops += 1;
            }
        }
        conn.execute(
            "UPDATE entities SET parent_id = ?1, updated_at = ?2 WHERE id = ?3 AND group_id = ?4",
            params![parent, now, id, group_id],
        )?;
    }
    if let Some(loc) = &req.location_id {
        conn.execute(
            "UPDATE entities SET location_id = ?1, updated_at = ?2 WHERE id = ?3 AND group_id = ?4",
            params![loc, now, id, group_id],
        )?;
    }
    if let Some(qty) = req.quantity {
        conn.execute(
            "UPDATE entities SET quantity = ?1, updated_at = ?2 WHERE id = ?3 AND group_id = ?4",
            params![qty, now, id, group_id],
        )?;
    }
    if let Some(archived) = req.archived {
        conn.execute(
            "UPDATE entities SET archived = ?1, updated_at = ?2 WHERE id = ?3 AND group_id = ?4",
            params![bool_sql(archived), now, id, group_id],
        )?;
    }
    if let Some(tags) = &req.tag_ids {
        super::taxonomy::set_entity_tags(conn, id, tags)?;
        conn.execute(
            "UPDATE entities SET updated_at = ?1 WHERE id = ?2 AND group_id = ?3",
            params![now, id, group_id],
        )?;
    }
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entities WHERE id = ?1 AND group_id = ?2",
        params![id, group_id],
        |r| r.get(0),
    )?;
    if n == 0 {
        return Err(AppError::NotFound("物品不存在".into()));
    }
    Ok(())
}

/// Delete an entity. Returns attachment file paths (relative) for cleanup.
pub fn delete(conn: &Connection, group_id: &str, id: &str) -> AppResult<Vec<String>> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entities WHERE id = ?1 AND group_id = ?2",
        params![id, group_id],
        |r| r.get(0),
    )?;
    if exists == 0 {
        return Err(AppError::NotFound("物品不存在".into()));
    }
    // ON DELETE CASCADE removes descendant entities too. Collect attachment
    // paths of the whole subtree so no orphan files are left on disk.
    let mut ids = vec![id.to_string()];
    let mut i = 0;
    while i < ids.len() {
        let cur = ids[i].clone();
        let children = query_all(
            conn,
            "SELECT id FROM entities WHERE parent_id = ?1 AND group_id = ?2",
            params![cur, group_id],
            |r| text(r, 0),
        )?;
        for c in children {
            if !ids.contains(&c) {
                ids.push(c);
            }
        }
        i += 1;
        if ids.len() > 10000 {
            break;
        }
    }
    let mut paths = Vec::new();
    for eid in &ids {
        paths.extend(query_all(
            conn,
            "SELECT path FROM attachments WHERE entity_id = ?1",
            params![eid],
            |r| text(r, 0),
        )?);
    }
    // explicit tag cleanup (join table has no cascade through both FKs in all builds)
    for eid in &ids {
        conn.execute("DELETE FROM entity_tags WHERE entity_id = ?1", params![eid])?;
        conn.execute("DELETE FROM maintenance_entries WHERE entity_id = ?1", params![eid])?;
    }
    conn.execute("DELETE FROM entities WHERE id = ?1", params![id])?;
    Ok(paths)
}

/// Duplicate an entity (tags + optional fields/maintenance/attachments metadata).
pub fn duplicate(
    conn: &Connection,
    group_id: &str,
    source_id: &str,
    new_id: &str,
    opts: &common::dto::DuplicateOptions,
) -> AppResult<Vec<(String, String)>> {
    let now = super::now();
    let src = query_one_str(
        conn,
        &format!("SELECT {COLS}{DETAIL_EXTRA} {FROM} WHERE e.group_id = ?1 AND e.id = ?2"),
        &[group_id.to_string(), source_id.to_string()],
        map_row,
    )?
    .ok_or_else(|| AppError::NotFound("物品不存在".into()))?;

    let name = if opts.copy_prefix.is_empty() {
        src.name.clone()
    } else {
        format!("{}{}", opts.copy_prefix, src.name)
    };

    conn.execute(
        "INSERT INTO entities (id, group_id, name, description, quantity, min_quantity,
                               asset_id, archived, insured, notes, parent_id, location_id,
                               sync_child_entity_locations, entity_type_id, serial_number,
                               model_number, manufacturer, lifetime_warranty, warranty_expires,
                               warranty_details, purchase_date, purchase_from, purchase_price,
                               sold_date, sold_to, sold_price, sold_notes, reminder_date,
                               reminder_note, created_at, updated_at)
         SELECT ?1, group_id, ?2, description, quantity, min_quantity, asset_id, archived,
                insured, notes, parent_id, location_id, sync_child_entity_locations,
                entity_type_id, serial_number, model_number, manufacturer, lifetime_warranty,
                warranty_expires, warranty_details, purchase_date, purchase_from,
                purchase_price, sold_date, sold_to, sold_price, sold_notes, reminder_date,
                reminder_note, ?3, ?3
           FROM entities WHERE id = ?4 AND group_id = ?5",
        params![new_id, name, now, source_id, group_id],
    )?;

    // tags
    let tag_ids: Vec<String> = query_all(
        conn,
        "SELECT tag_id FROM entity_tags WHERE entity_id = ?1",
        params![source_id],
        |r| text(r, 0),
    )?;
    super::taxonomy::set_entity_tags(conn, new_id, &tag_ids)?;

    if opts.copy_custom_fields {
        let mut fields = list_fields(conn, source_id)?;
        // Field rows use a global PRIMARY KEY (entity_fields.id); reusing the
        // source ids collides with the still-existing source rows. Regenerate.
        for f in &mut fields {
            f.id = uuid::Uuid::new_v4().to_string();
        }
        set_fields(conn, new_id, &fields)?;
    }
    if opts.copy_maintenance {
        conn.execute(
            "INSERT INTO maintenance_entries (id, entity_id, name, description, cost,
                                              completed_date, scheduled_date, created_at, updated_at)
             SELECT lower(hex(randomblob(16))), ?1, name, description, cost,
                    completed_date, scheduled_date, ?2, ?2
               FROM maintenance_entries WHERE entity_id = ?3",
            params![new_id, now, source_id],
        )?;
    }
    let mut copied_files = Vec::new();
    if opts.copy_attachments {
        let rows = query_all(
            conn,
            "SELECT id, type, title, file_name, path, mime_type, size_bytes, primary_photo
               FROM attachments WHERE entity_id = ?1",
            params![source_id],
            |r| {
                Ok::<_, rusqlite::Error>((
                    text(r, 0)?,
                    text(r, 1)?,
                    text(r, 2)?,
                    text(r, 3)?,
                    text(r, 4)?,
                    text(r, 5)?,
                    r.get::<_, i64>(6)?,
                    from_bool(r.get(7)?),
                ))
            },
        )?;
        for (_aid, kind, title, file_name, path, mime, size, primary) in rows {
            let new_path = path.replacen(&source_id, &new_id, 1);
            conn.execute(
                "INSERT INTO attachments (id, entity_id, type, title, file_name, path,
                                          mime_type, size_bytes, primary_photo, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
                params![uuid::Uuid::new_v4().to_string(), new_id, kind, title, file_name, new_path, mime, size, bool_sql(primary), now],
            )?;
            copied_files.push((path, new_path));
        }
    }
    Ok(copied_files)
}

/// Breadcrumb from root to the entity itself.
pub fn path(conn: &Connection, group_id: &str, id: &str) -> AppResult<Vec<EntityPath>> {
    let mut out = Vec::new();
    let mut cur = Some(id.to_string());
    let mut seen = HashSet::new();
    while let Some(cid) = cur {
        if !seen.insert(cid.clone()) {
            break; // cycle guard
        }
        let row: Option<(String, String, Option<String>)> = query_one_str(
            conn,
            "SELECT id, name, parent_id FROM entities WHERE id = ?1 AND group_id = ?2",
            &[cid.clone(), group_id.to_string()],
            |r| Ok::<_, rusqlite::Error>((text(r, 0)?, text(r, 1)?, r.get(2)?)),
        )?;
        match row {
            Some((pid, name, parent)) => {
                out.push(EntityPath { id: pid, name });
                cur = parent;
            }
            None => break,
        }
    }
    out.reverse();
    Ok(out)
}

/// Location tree with per-location item counts.
pub fn tree(conn: &Connection, group_id: &str) -> AppResult<Vec<TreeItem>> {
    let rows = query_all(
        conn,
        "SELECT e.id, e.name, et.icon, et.is_location, e.parent_id, e.archived
           FROM entities e JOIN entity_types et ON et.id = e.entity_type_id
          WHERE e.group_id = ?1 AND et.is_location = 1
          ORDER BY e.name COLLATE NOCASE",
        params![group_id],
        |r| {
            Ok::<_, rusqlite::Error>((
                text(r, 0)?,
                text(r, 1)?,
                text(r, 2)?,
                from_bool(r.get(3)?),
                r.get::<_, Option<String>>(4)?,
                from_bool(r.get(5)?),
            ))
        },
    )?;
    let counts = item_counts(conn, group_id)?;

    struct Node {
        id: String,
        name: String,
        icon: String,
        is_location: bool,
        parent_id: Option<String>,
        archived: bool,
    }
    let nodes: Vec<Node> = rows
        .into_iter()
        .map(|(id, name, icon, is_location, parent_id, archived)| Node {
            id,
            name,
            icon,
            is_location,
            parent_id,
            archived,
        })
        .collect();

    // children lookup, only attaching to location parents
    let location_ids: HashSet<&str> = nodes.iter().filter(|n| n.is_location).map(|n| n.id.as_str()).collect();
    let mut children: HashMap<&str, Vec<&Node>> = HashMap::new();
    for n in &nodes {
        let parent = n.parent_id.as_deref().filter(|p| location_ids.contains(p));
        children.entry(parent.unwrap_or("")).or_default().push(n);
    }

    fn build(
        key: &str,
        children: &mut HashMap<&str, Vec<&Node>>,
        counts: &HashMap<String, i64>,
    ) -> Vec<TreeItem> {
        let Some(kids) = children.remove(key) else {
            return Vec::new();
        };
        kids.iter()
            .map(|n| TreeItem {
                id: n.id.clone(),
                name: n.name.clone(),
                icon: n.icon.clone(),
                is_location: n.is_location,
                item_count: counts.get(&n.id).copied().unwrap_or(0),
                children: build(&n.id, children, counts),
            })
            .collect()
    }
    Ok(build("", &mut children, &counts))
}

/// Attachment ids + paths of an entity (used for file cleanup on delete).
pub fn attachment_paths(conn: &Connection, entity_id: &str) -> AppResult<Vec<(String, String)>> {
    query_all(
        conn,
        "SELECT id, path FROM attachments WHERE entity_id = ?1",
        params![entity_id],
        |r| Ok::<_, rusqlite::Error>((text(r, 0)?, text(r, 1)?)),
    )
}

/// Does the entity exist in this group (id + tenant check)?
pub fn entity_exists(conn: &Connection, group_id: &str, id: &str) -> AppResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entities WHERE id = ?1 AND group_id = ?2",
        params![id, group_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Check that referenced type/parent/tags belong to the group.
/// Validate all references carried by a partial update, including the
/// nested `Option<Option<String>>` parent/location fields. Only fields that
/// are actually being set to an id are checked; clearing to null is fine.
pub fn validate_patch_refs(
    conn: &Connection,
    group_id: &str,
    req: &common::dto::EntityPatch,
) -> AppResult<()> {
    if let Some(type_id) = &req.entity_type_id {
        let ok: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entity_types WHERE id = ?1 AND group_id = ?2",
            params![type_id, group_id],
            |r| r.get(0),
        )?;
        if ok == 0 {
            return Err(AppError::Validation("物品类型不存在".into()));
        }
    }
    // Option<Option<..>>: outer None = not touched; Some(None) = clear to null.
    for pid in req
        .parent_id
        .iter()
        .flatten()
        .chain(req.location_id.iter().flatten())
    {
        let ok: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entities WHERE id = ?1 AND group_id = ?2",
            params![pid, group_id],
            |r| r.get(0),
        )?;
        if ok == 0 {
            return Err(AppError::Validation("上级位置不存在".into()));
        }
    }
    for tid in req.tag_ids.iter().flatten() {
        let ok: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tags WHERE id = ?1 AND group_id = ?2",
            params![tid, group_id],
            |r| r.get(0),
        )?;
        if ok == 0 {
            return Err(AppError::Validation("标签不存在".into()));
        }
    }
    Ok(())
}

pub fn validate_refs(
    conn: &Connection,
    group_id: &str,
    type_id: &str,
    parent_id: &Option<String>,
    location_id: &Option<String>,
    tag_ids: &[String],
) -> AppResult<()> {
    let type_ok: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entity_types WHERE id = ?1 AND group_id = ?2",
        params![type_id, group_id],
        |r| r.get(0),
    )?;
    if type_ok == 0 {
        return Err(AppError::Validation("物品类型不存在".into()));
    }
    for pid in [parent_id, location_id].into_iter().flatten() {
        let ok: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entities WHERE id = ?1 AND group_id = ?2",
            params![pid, group_id],
            |r| r.get(0),
        )?;
        if ok == 0 {
            return Err(AppError::Validation("上级位置不存在".into()));
        }
    }
    for tid in tag_ids {
        let ok: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tags WHERE id = ?1 AND group_id = ?2",
            params![tid, group_id],
            |r| r.get(0),
        )?;
        if ok == 0 {
            return Err(AppError::Validation("标签不存在".into()));
        }
    }
    Ok(())
}

/// Maintenance entries of one entity (used by detail aggregation elsewhere).
pub fn maintenance_of(conn: &Connection, entity_id: &str) -> AppResult<Vec<MaintenanceEntry>> {
    super::maintenance::list(conn, entity_id, "all")
}

/// Re-export for handlers that need the summary type in scope.
pub type Summary = EntitySummary;
pub type Attachment = ItemAttachment;

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
        let (loc_type, item_type) = super::super::taxonomy::seed_defaults(&conn, "g1").unwrap();
        conn.execute(
            "INSERT INTO entities (id, group_id, name, entity_type_id, created_at, updated_at)
             VALUES ('loc1', 'g1', '客厅', ?1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![loc_type],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entities (id, group_id, name, entity_type_id, created_at, updated_at)
             VALUES ('loc2', 'g1', '卧室', ?1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![loc_type],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entities (id, group_id, name, entity_type_id, parent_id, location_id, quantity, min_quantity, created_at, updated_at)
             VALUES ('it1', 'g1', '电视', ?1, 'loc1', 'loc1', 1, 0, '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z')",
            params![item_type],
        )
        .unwrap();
        conn
    }

    #[test]
    fn list_filters_and_paginates() {
        let conn = setup();
        let q = EntitiesQuery::default();
        let page = list(&conn, "g1", &q).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 3);

        let q = EntitiesQuery { is_location: Some(true), ..Default::default() };
        let page = list(&conn, "g1", &q).unwrap();
        assert_eq!(page.total, 2);

        let q = EntitiesQuery { q: "电视".into(), ..Default::default() };
        let page = list(&conn, "g1", &q).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "it1");

        let q = EntitiesQuery { page_size: 2, page: 2, ..Default::default() };
        let page = list(&conn, "g1", &q).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.total, 3);
    }

    #[test]
    fn detail_roundtrip() {
        let conn = setup();
        let out = get(&conn, "g1", "it1").unwrap();
        assert_eq!(out.summary.name, "电视");
        assert_eq!(out.summary.location.as_ref().map(|l| l.id.as_str()), Some("loc1"));
        assert_eq!(out.summary.entity_type.as_ref().map(|t| t.name.as_str()), Some("Item"));
    }

    #[test]
    fn location_tree_counts_items() {
        let conn = setup();
        let tree = tree(&conn, "g1").unwrap();
        assert_eq!(tree.len(), 2);
        let living = tree.iter().find(|t| t.name == "客厅").unwrap();
        assert_eq!(living.item_count, 1);
        let bedroom = tree.iter().find(|t| t.name == "卧室").unwrap();
        assert_eq!(bedroom.item_count, 0);
    }

    #[test]
    fn delete_returns_attachment_paths() {
        let conn = setup();
        conn.execute(
            "INSERT INTO attachments (id, entity_id, path, created_at, updated_at)
             VALUES ('a1', 'it1', 'it1/a1.jpg', '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z')",
            [],
        )
        .unwrap();
        let paths = delete(&conn, "g1", "it1").unwrap();
        assert_eq!(paths, vec!["it1/a1.jpg".to_string()]);
        assert!(get(&conn, "g1", "it1").is_err());
    }

    #[test]
    fn patch_updates_selected_fields() {
        let conn = setup();
        let req = common::dto::EntityPatch { quantity: Some(5.0), archived: Some(true), ..Default::default() };
        patch(&conn, "g1", "it1", &req).unwrap();
        let out = get(&conn, "g1", "it1").unwrap();
        assert!((out.summary.quantity - 5.0).abs() < f64::EPSILON);
        assert!(out.summary.archived);
    }

    #[test]
    fn breadcrumbs_walk_up() {
        let conn = setup();
        let p = path(&conn, "g1", "it1").unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].name, "客厅");
        assert_eq!(p[1].name, "电视");
    }

    #[test]
    fn duplicate_with_maintenance_and_fields_succeeds() {
        let conn = setup();
        conn.execute(
            "INSERT INTO maintenance_entries (id, entity_id, name, created_at, updated_at)
             VALUES ('m1', 'it1', '保养', '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z')",
            [],
        )
        .unwrap();
        let f = EntityFieldData {
            id: "f1".into(),
            name: "颜色".into(),
            kind: "text".into(),
            text_value: "黑".into(),
            number_value: 0.0,
            boolean_value: false,
        };
        set_fields(&conn, "it1", &[f]).unwrap();

        let opts = common::dto::DuplicateOptions {
            copy_maintenance: true,
            copy_custom_fields: true,
            copy_attachments: false,
            copy_prefix: "副本 ".into(),
        };
        let copied = duplicate(&conn, "g1", "it1", "copy1", &opts).unwrap();
        assert!(copied.is_empty()); // no attachments requested

        let out = get(&conn, "g1", "copy1").unwrap();
        assert_eq!(out.summary.name, "副本 电视");
        assert_eq!(out.maintenance_count, 1);
        assert_eq!(out.fields.len(), 1);
        assert_ne!(out.fields[0].id, "f1"); // copied field got a fresh PK
        assert_eq!(out.fields[0].name, "颜色");

        let source_fields = list_fields(&conn, "it1").unwrap();
        assert_eq!(source_fields[0].id, "f1"); // source unchanged

        let m: i64 = conn.query_row(
            "SELECT COUNT(*) FROM maintenance_entries WHERE entity_id = 'copy1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(m, 1);
    }

    #[test]
    fn duplicate_invalidates_foreign_refs() {
        let conn = setup();
        // A second group's type/id must be rejected by patch validation.
        conn.execute(
            "INSERT INTO groups (id, name, currency, created_at, updated_at)
             VALUES ('g2', 'Other', 'CNY', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let foreign_id = "foreign-type";
        conn.execute(
            "INSERT INTO entity_types (id, group_id, name, created_at, updated_at)
             VALUES (?1, 'g2', 'Foreign', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![foreign_id],
        )
        .unwrap();
        let req = common::dto::EntityPatch {
            entity_type_id: Some(foreign_id.into()),
            ..Default::default()
        };
        assert!(validate_patch_refs(&conn, "g1", &req).is_err());
    }

    #[test]
    fn search_wildcards_are_literal() {
        let conn = setup();
        let q = EntitiesQuery { q: "%".into(), ..Default::default() };
        let page = list(&conn, "g1", &q).unwrap();
        assert_eq!(page.total, 0, "a literal % must not match everything");
    }
}
