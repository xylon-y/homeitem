//! CSV import/export data access.
//!
//! Wire columns mirror upstream HomeBox (`HB.*` prefix) so exports can be
//! re-imported there and vice versa.

use std::collections::HashMap;

use rusqlite::{params, Connection};

use super::{bool_sql, from_bool, query_all, query_optional, text};
use crate::error::AppResult;

/// One exported row: the entity's scalar columns plus its tags and custom fields.
pub struct ExportRow {
    pub id: String,
    pub import_ref: String,
    pub name: String,
    pub description: String,
    pub quantity: f64,
    pub min_quantity: f64,
    pub asset_id: i64,
    pub insured: bool,
    pub serial_number: String,
    pub model_number: String,
    pub manufacturer: String,
    pub notes: String,
    pub purchase_from: String,
    pub purchase_price: f64,
    pub purchase_date: Option<String>,
    pub lifetime_warranty: bool,
    pub warranty_expires: Option<String>,
    pub warranty_details: String,
    pub sold_to: String,
    pub sold_price: f64,
    pub sold_date: Option<String>,
    pub sold_notes: String,
    pub location_name: String,
    pub tags: Vec<String>,
    pub fields: Vec<(String, String)>,
}

const EXPORT_COLS: &str = "e.id, e.import_ref, e.name, e.description, e.quantity, e.min_quantity,
    e.asset_id, e.insured, e.serial_number, e.model_number, e.manufacturer, e.notes,
    e.purchase_from, e.purchase_price, e.purchase_date, e.lifetime_warranty,
    e.warranty_expires, e.warranty_details, e.sold_to, e.sold_price, e.sold_date, e.sold_notes";

/// All non-location items of a group for export.
pub fn export_rows(conn: &Connection, group_id: &str) -> AppResult<Vec<ExportRow>> {
    let rows = query_all(
        conn,
        &format!(
            "SELECT {EXPORT_COLS}, COALESCE(loc.name, '')
               FROM entities e
               JOIN entity_types et ON et.id = e.entity_type_id
               LEFT JOIN entities loc ON loc.id = e.location_id
              WHERE e.group_id = ?1 AND et.is_location = 0
              ORDER BY e.name COLLATE NOCASE"
        ),
        params![group_id],
        |r| {
            Ok::<_, rusqlite::Error>((
                text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?,
                r.get::<_, f64>(4)?, r.get::<_, f64>(5)?, r.get::<_, i64>(6)?,
                from_bool(r.get(7)?),
                text(r, 8)?, text(r, 9)?, text(r, 10)?, text(r, 11)?,
                text(r, 12)?, r.get::<_, f64>(13)?,
                r.get::<_, Option<String>>(14)?, from_bool(r.get(15)?),
                r.get::<_, Option<String>>(16)?, text(r, 17)?,
                text(r, 18)?, r.get::<_, f64>(19)?,
                r.get::<_, Option<String>>(20)?, text(r, 21)?,
                text(r, 22)?,
            ))
        },
    )?;

    // tags and custom fields, batched (avoid N+1)
    let mut tags: HashMap<String, Vec<String>> = HashMap::new();
    let tag_rows = query_all(
        conn,
        "SELECT et.entity_id, t.name FROM entity_tags et
           JOIN tags t ON t.id = et.tag_id
           JOIN entities e ON e.id = et.entity_id
          WHERE e.group_id = ?1 ORDER BY t.name",
        params![group_id],
        |r| Ok::<_, rusqlite::Error>((text(r, 0)?, text(r, 1)?)),
    )?;
    for (eid, name) in tag_rows {
        tags.entry(eid).or_default().push(name);
    }

    let mut fields: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let field_rows = query_all(
        conn,
        "SELECT ef.entity_id, ef.name, ef.text_value, ef.number_value, ef.boolean_value
           FROM entity_fields ef
           JOIN entities e ON e.id = ef.entity_id
          WHERE e.group_id = ?1 ORDER BY ef.name",
        params![group_id],
        |r| {
            Ok::<_, rusqlite::Error>((
                text(r, 0)?, text(r, 1)?, text(r, 2)?,
                r.get::<_, f64>(3)?, from_bool(r.get(4)?),
            ))
        },
    )?;
    for (eid, name, text_v, num_v, bool_v) in field_rows {
        let value = if !text_v.is_empty() {
            text_v
        } else if num_v != 0.0 {
            num_v.to_string()
        } else if bool_v {
            "true".to_string()
        } else {
            String::new()
        };
        fields.entry(eid).or_default().push((name, value));
    }

    Ok(rows
        .into_iter()
        .map(|r| {
            let id = r.0.clone();
            ExportRow {
            id: r.0,
            import_ref: r.1,
            name: r.2,
            description: r.3,
            quantity: r.4,
            min_quantity: r.5,
            asset_id: r.6,
            insured: r.7,
            serial_number: r.8,
            model_number: r.9,
            manufacturer: r.10,
            notes: r.11,
            purchase_from: r.12,
            purchase_price: r.13,
            purchase_date: r.14,
            lifetime_warranty: r.15,
            warranty_expires: r.16,
            warranty_details: r.17,
            sold_to: r.18,
            sold_price: r.19,
            sold_date: r.20,
            sold_notes: r.21,
            location_name: r.22,
            tags: tags.remove(&id).unwrap_or_default(),
            fields: fields.remove(&id).unwrap_or_default(),
            }
        })
        .collect())
}

/// Resolve or create a location from a path like `Home / Office / Desk`.
/// Returns the leaf location entity id.
pub fn resolve_location_path(
    conn: &Connection,
    group_id: &str,
    path: &str,
) -> AppResult<Option<String>> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(None);
    }
    let location_type = conn.query_row(
        "SELECT id FROM entity_types WHERE group_id = ?1 AND is_location = 1 ORDER BY created_at LIMIT 1",
        params![group_id],
        |r| text(r, 0),
    )?;
    let now = super::now();
    let mut parent: Option<String> = None;
    let mut leaf = String::new();
    for segment in path.split('/').map(str::trim).filter(|s| !s.is_empty()) {
        let found: Option<String> = query_optional(
            conn,
            "SELECT id FROM entities
              WHERE group_id = ?1 AND name = ?2 COLLATE NOCASE
                AND entity_type_id = ?3
                AND parent_id IS ?4
              LIMIT 1",
            &[&group_id, &segment, &location_type, &parent],
            |r| text(r, 0),
        )?;
        leaf = match found {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO entities
                       (id, group_id, name, quantity, min_quantity, entity_type_id, parent_id, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 1, 0, ?4, ?5, ?6, ?6)",
                    params![id, group_id, segment, location_type, parent, now],
                )?;
                id
            }
        };
        parent = Some(leaf.clone());
    }
    Ok(if leaf.is_empty() { None } else { Some(leaf) })
}

/// Resolve or create a tag by name; returns its id.
pub fn resolve_tag(conn: &Connection, group_id: &str, name: &str) -> AppResult<String> {
    let name = name.trim();
    let found: Option<String> = query_optional(
        conn,
        "SELECT id FROM tags WHERE group_id = ?1 AND name = ?2 COLLATE NOCASE LIMIT 1",
        &[&group_id, &name],
        |r| text(r, 0),
    )?;
    if let Some(id) = found {
        return Ok(id);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = super::now();
    conn.execute(
        "INSERT INTO tags (id, group_id, name, description, color, icon, created_at, updated_at)
         VALUES (?1, ?2, ?3, '', '', '', ?4, ?4)",
        params![id, group_id, name, now],
    )?;
    Ok(id)
}

/// A row parsed from CSV, ready to upsert.
pub struct ImportRow {
    pub import_ref: String,
    pub name: String,
    pub description: String,
    pub quantity: f64,
    pub min_quantity: f64,
    pub asset_id: i64,
    pub insured: bool,
    pub serial_number: String,
    pub model_number: String,
    pub manufacturer: String,
    pub notes: String,
    pub purchase_from: String,
    pub purchase_price: f64,
    pub purchase_date: Option<String>,
    pub lifetime_warranty: bool,
    pub warranty_expires: Option<String>,
    pub warranty_details: String,
    pub sold_to: String,
    pub sold_price: f64,
    pub sold_date: Option<String>,
    pub sold_notes: String,
    pub location_path: String,
    pub tags: Vec<String>,
    pub fields: Vec<(String, String)>,
}

impl Default for ImportRow {
    fn default() -> Self {
        Self {
            import_ref: String::new(), name: String::new(), description: String::new(),
            quantity: 1.0, min_quantity: 0.0, asset_id: 0, insured: false,
            serial_number: String::new(), model_number: String::new(),
            manufacturer: String::new(), notes: String::new(),
            purchase_from: String::new(), purchase_price: 0.0, purchase_date: None,
            lifetime_warranty: false, warranty_expires: None, warranty_details: String::new(),
            sold_to: String::new(), sold_price: 0.0, sold_date: None, sold_notes: String::new(),
            location_path: String::new(), tags: Vec::new(), fields: Vec::new(),
        }
    }
}

/// Upsert one imported row. Returns (id, created) where created=true for new items.
pub fn upsert_row(
    conn: &Connection,
    group_id: &str,
    row: &ImportRow,
) -> AppResult<(String, bool)> {
    let now = super::now();
    let existing = super::entities::find_by_import_ref(conn, group_id, &row.import_ref)?;
    let location_id = super::csvio::resolve_location_path(conn, group_id, &row.location_path)?;
    let tag_ids: Vec<String> = row
        .tags
        .iter()
        .map(|t| super::csvio::resolve_tag(conn, group_id, t))
        .collect::<AppResult<Vec<_>>>()?;

    let (id, created) = match existing {
        Some(id) => (id, false),
        None => (uuid::Uuid::new_v4().to_string(), true),
    };

    // All items import as the group's default "Item" type.
    let item_type = conn.query_row(
        "SELECT id FROM entity_types WHERE group_id = ?1 AND is_location = 0 ORDER BY created_at LIMIT 1",
        params![group_id],
        |r| text(r, 0),
    )?;

    if created {
        conn.execute(
            "INSERT INTO entities
               (id, group_id, name, description, quantity, min_quantity, asset_id, insured,
                serial_number, model_number, manufacturer, notes, entity_type_id, location_id,
                purchase_from, purchase_price, purchase_date, lifetime_warranty,
                warranty_expires, warranty_details, sold_to, sold_price, sold_date, sold_notes,
                import_ref, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,
                     ?19,?20,?21,?22,?23,?24,?25,?26,?26)",
            params![
                id, group_id, row.name, row.description, row.quantity, row.min_quantity,
                row.asset_id, bool_sql(row.insured), row.serial_number, row.model_number,
                row.manufacturer, row.notes, item_type, location_id,
                row.purchase_from, row.purchase_price, row.purchase_date, bool_sql(row.lifetime_warranty),
                row.warranty_expires, row.warranty_details, row.sold_to, row.sold_price,
                row.sold_date, row.sold_notes, row.import_ref, now
            ],
        )?;
    } else {
        conn.execute(
            "UPDATE entities SET
                name=?1, description=?2, quantity=?3, min_quantity=?4, asset_id=?5, insured=?6,
                serial_number=?7, model_number=?8, manufacturer=?9, notes=?10, location_id=?11,
                purchase_from=?12, purchase_price=?13, purchase_date=?14, lifetime_warranty=?15,
                warranty_expires=?16, warranty_details=?17, sold_to=?18, sold_price=?19,
                sold_date=?20, sold_notes=?21, import_ref=?22, updated_at=?23
              WHERE id=?24 AND group_id=?25",
            params![
                row.name, row.description, row.quantity, row.min_quantity, row.asset_id,
                bool_sql(row.insured), row.serial_number, row.model_number, row.manufacturer,
                row.notes, location_id, row.purchase_from, row.purchase_price, row.purchase_date,
                bool_sql(row.lifetime_warranty), row.warranty_expires, row.warranty_details,
                row.sold_to, row.sold_price, row.sold_date, row.sold_notes, row.import_ref,
                now, id, group_id
            ],
        )?;
    }

    super::taxonomy::set_entity_tags(conn, &id, &tag_ids)?;

    // custom fields: replace with parsed HB.field.* values
    if !row.fields.is_empty() {
        conn.execute("DELETE FROM entity_fields WHERE entity_id = ?1", params![id])?;
        for (name, value) in &row.fields {
            if value.is_empty() {
                continue;
            }
            conn.execute(
                "INSERT INTO entity_fields (id, entity_id, name, type, text_value, number_value, boolean_value)
                 VALUES (?1, ?2, ?3, 'text', ?4, 0, 0)",
                params![uuid::Uuid::new_v4().to_string(), id, name, value],
            )?;
        }
    }

    Ok((id, created))
}
