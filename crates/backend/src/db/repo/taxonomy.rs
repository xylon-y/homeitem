//! Tags and entity types.

use common::dto::{
    EntityTypeCreate, EntityTypeOut, EntityTypeUpdate, TagCreate, TagOut, TagSummary, TagUpdate,
};
use rusqlite::{params, Connection};

use super::{bool_sql, from_bool, query_all, query_optional, text, ts_or_epoch};
use crate::error::{AppError, AppResult};

// ---------------------------------------------------------------------------
// entity types
// ---------------------------------------------------------------------------

const TYPE_COLS: &str = "id, name, description, icon, is_location, created_at, updated_at";

fn map_type(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntityTypeOut> {
    Ok(EntityTypeOut {
        id: text(row, 0)?,
        name: text(row, 1)?,
        description: text(row, 2)?,
        icon: text(row, 3)?,
        is_location: from_bool(row.get(4)?),
        created_at: ts_or_epoch(row.get::<_, Option<String>>(5)?.as_deref()),
        updated_at: ts_or_epoch(row.get::<_, Option<String>>(6)?.as_deref()),
    })
}

/// All entity types of a group; `is_location` optionally filters.
pub fn list_types(
    conn: &Connection,
    group_id: &str,
    is_location: Option<bool>,
) -> AppResult<Vec<EntityTypeOut>> {
    let mut sql = format!("SELECT {TYPE_COLS} FROM entity_types WHERE group_id = ?1");
    let loc_param;
    match is_location {
        Some(v) => {
            sql.push_str(" AND is_location = ?2");
            loc_param = Some(bool_sql(v));
        }
        None => loc_param = None,
    }
    sql.push_str(" ORDER BY is_location DESC, name");
    let params: Vec<&dyn rusqlite::ToSql> = match &loc_param {
        Some(v) => vec![&group_id, v],
        None => vec![&group_id],
    };
    query_all(conn, &sql, &params, map_type)
}

/// One entity type by id (scoped to the group).
pub fn find_type(conn: &Connection, group_id: &str, id: &str) -> AppResult<Option<EntityTypeOut>> {
    let sql = format!("SELECT {TYPE_COLS} FROM entity_types WHERE group_id = ?1 AND id = ?2");
    query_optional(conn, &sql, &[&group_id, &id], map_type)
}

/// Create an entity type; returns the new id.
pub fn create_type(
    conn: &Connection,
    group_id: &str,
    id: &str,
    req: &EntityTypeCreate,
) -> AppResult<()> {
    let now = super::now();
    let dup: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entity_types WHERE group_id = ?1 AND name = ?2 COLLATE NOCASE",
        params![group_id, req.name],
        |r| r.get(0),
    )?;
    if dup > 0 {
        return Err(AppError::Conflict(format!("类型已存在: {}", req.name)));
    }
    conn.execute(
        "INSERT INTO entity_types (id, group_id, name, description, icon, is_location, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            id,
            group_id,
            req.name,
            req.description,
            req.icon,
            bool_sql(req.is_location),
            now
        ],
    )?;
    Ok(())
}

/// Update an entity type.
pub fn update_type(
    conn: &Connection,
    group_id: &str,
    id: &str,
    req: &EntityTypeUpdate,
) -> AppResult<()> {
    let now = super::now();
    let n = conn.execute(
        "UPDATE entity_types SET name = ?1, description = ?2, icon = ?3, is_location = ?4, updated_at = ?5
          WHERE id = ?6 AND group_id = ?7",
        params![
            req.name,
            req.description,
            req.icon,
            bool_sql(req.is_location),
            now,
            id,
            group_id
        ],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("entity type not found".into()));
    }
    Ok(())
}

/// Delete an entity type; fails with `Conflict` while entities reference it.
pub fn delete_type(conn: &Connection, group_id: &str, id: &str) -> AppResult<()> {
    let refs: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entities WHERE entity_type_id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if refs > 0 {
        return Err(AppError::Conflict(format!("仍有 {refs} 个物品在使用该类型")));
    }
    let n = conn.execute(
        "DELETE FROM entity_types WHERE id = ?1 AND group_id = ?2",
        params![id, group_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("entity type not found".into()));
    }
    Ok(())
}

/// Seed the default Location/Item types during setup. Returns (location_id, item_id).
pub fn seed_defaults(conn: &Connection, group_id: &str) -> AppResult<(String, String)> {
    let now = super::now();
    let loc_id = uuid::Uuid::new_v4().to_string();
    let item_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO entity_types (id, group_id, name, description, icon, is_location, created_at, updated_at)
         VALUES (?1, ?2, 'Location', '位置', 'location_on', 1, ?3, ?3)",
        params![loc_id, group_id, now],
    )?;
    conn.execute(
        "INSERT INTO entity_types (id, group_id, name, description, icon, is_location, created_at, updated_at)
         VALUES (?1, ?2, 'Item', '物品', 'inventory_2', 0, ?3, ?3)",
        params![item_id, group_id, now],
    )?;
    Ok((loc_id, item_id))
}

// ---------------------------------------------------------------------------
// tags
// ---------------------------------------------------------------------------

const TAG_COLS: &str = "id, name, description, color, icon, parent_id";

fn map_tag_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<TagSummary> {
    Ok(TagSummary {
        id: text(row, 0)?,
        name: text(row, 1)?,
        description: text(row, 2)?,
        color: text(row, 3)?,
        icon: text(row, 4)?,
        parent_id: row.get(5)?,
        item_count: None,
    })
}

/// Flat list of all tags in a group (optionally with per-tag item counts).
pub fn list_tags(
    conn: &Connection,
    group_id: &str,
    with_counts: bool,
) -> AppResult<Vec<TagSummary>> {
    let base = if with_counts {
        "SELECT t.id, t.name, t.description, t.color, t.icon, t.parent_id,
                (SELECT COUNT(*) FROM entity_tags et WHERE et.tag_id = t.id)
           FROM tags t WHERE t.group_id = ?1 ORDER BY t.name"
    } else {
        "SELECT t.id, t.name, t.description, t.color, t.icon, t.parent_id, NULL
           FROM tags t WHERE t.group_id = ?1 ORDER BY t.name"
    };
    query_all(conn, base, &[&group_id], |row| {
        let mut tag = map_tag_summary(row)?;
        tag.item_count = row.get::<_, Option<i64>>(6)?;
        Ok(tag)
    })
}

/// Load tags for a set of entity ids, keyed by entity id.
pub fn tags_for_entities(
    conn: &Connection,
    entity_ids: &[String],
) -> AppResult<std::collections::HashMap<String, Vec<TagSummary>>> {
    let mut out: std::collections::HashMap<String, Vec<TagSummary>> =
        std::collections::HashMap::new();
    if entity_ids.is_empty() {
        return Ok(out);
    }
    let placeholders = (1..=entity_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT et.entity_id, t.id, t.name, t.description, t.color, t.icon, t.parent_id
           FROM entity_tags et JOIN tags t ON t.id = et.tag_id
          WHERE et.entity_id IN ({placeholders}) ORDER BY t.name"
    );
    let params: Vec<&dyn rusqlite::ToSql> = entity_ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
    let rows = query_all(conn, &sql, &params, |row| {
        Ok::<_, rusqlite::Error>((
            text(row, 0)?,
            TagSummary {
                id: text(row, 1)?,
                name: text(row, 2)?,
                description: text(row, 3)?,
                color: text(row, 4)?,
                icon: text(row, 5)?,
                parent_id: row.get(6)?,
                item_count: None,
            },
        ))
    })?;
    for (entity_id, tag) in rows {
        out.entry(entity_id).or_default().push(tag);
    }
    Ok(out)
}

/// Tags attached to one entity.
pub fn tags_for_entity(conn: &Connection, entity_id: &str) -> AppResult<Vec<TagSummary>> {
    query_all(
        conn,
        "SELECT t.id, t.name, t.description, t.color, t.icon, t.parent_id
           FROM entity_tags et JOIN tags t ON t.id = et.tag_id
          WHERE et.entity_id = ?1 ORDER BY t.name",
        &[&entity_id],
        map_tag_summary,
    )
}

/// One tag by id.
pub fn find_tag(conn: &Connection, group_id: &str, id: &str) -> AppResult<Option<TagSummary>> {
    query_optional(
        conn,
        "SELECT id, name, description, color, icon, parent_id FROM tags
          WHERE group_id = ?1 AND id = ?2",
        &[&group_id, &id],
        map_tag_summary,
    )
}

/// Create a tag; returns its id.
pub fn create_tag(conn: &Connection, group_id: &str, id: &str, req: &TagCreate) -> AppResult<()> {
    let now = super::now();
    let dup: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tags WHERE group_id = ?1 AND name = ?2 COLLATE NOCASE",
        params![group_id, req.name],
        |r| r.get(0),
    )?;
    if dup > 0 {
        return Err(AppError::Conflict(format!("标签已存在: {}", req.name)));
    }
    conn.execute(
        "INSERT INTO tags (id, group_id, name, description, color, icon, parent_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![id, group_id, req.name, req.description, req.color, req.icon, req.parent_id, now],
    )?;
    Ok(())
}

/// Update a tag.
pub fn update_tag(
    conn: &Connection,
    group_id: &str,
    id: &str,
    req: &TagUpdate,
) -> AppResult<()> {
    let now = super::now();
    let n = conn.execute(
        "UPDATE tags SET name = ?1, description = ?2, color = ?3, icon = ?4, parent_id = ?5, updated_at = ?6
          WHERE id = ?7 AND group_id = ?8",
        params![req.name, req.description, req.color, req.icon, req.parent_id, now, id, group_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("tag not found".into()));
    }
    Ok(())
}

/// Delete a tag (entity_tags rows cascade).
pub fn delete_tag(conn: &Connection, group_id: &str, id: &str) -> AppResult<()> {
    let n = conn.execute(
        "DELETE FROM tags WHERE id = ?1 AND group_id = ?2",
        params![id, group_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("tag not found".into()));
    }
    // clear parent references that pointed at it
    conn.execute(
        "UPDATE tags SET parent_id = NULL WHERE parent_id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Replace the tag set of an entity (used by create/update).
pub fn set_entity_tags(conn: &Connection, entity_id: &str, tag_ids: &[String]) -> AppResult<()> {
    conn.execute("DELETE FROM entity_tags WHERE entity_id = ?1", params![entity_id])?;
    for tag in tag_ids {
        conn.execute(
            "INSERT OR IGNORE INTO entity_tags (entity_id, tag_id) VALUES (?1, ?2)",
            params![entity_id, tag],
        )?;
    }
    Ok(())
}

/// Build the nested `TagOut` tree from a flat list.
///
/// Tags whose parent is missing from the flat list (or `parent_id = NULL`)
/// become roots. Timestamps are not part of `TagSummary`, so they are filled
/// with the epoch placeholder — list endpoints only need the tree structure.
pub fn tag_tree(mut flat: Vec<TagSummary>, with_counts: bool) -> Vec<TagOut> {
    flat.sort_by(|a, b| a.name.cmp(&b.name));
    let mut children: std::collections::HashMap<Option<String>, Vec<TagSummary>> =
        std::collections::HashMap::new();
    for tag in &flat {
        children.entry(tag.parent_id.clone()).or_default().push(tag.clone());
    }
    fn build(
        parent: &Option<String>,
        children: &mut std::collections::HashMap<Option<String>, Vec<TagSummary>>,
        with_counts: bool,
    ) -> Vec<TagOut> {
        let Some(kids) = children.remove(parent) else {
            return Vec::new();
        };
        kids.into_iter()
            .map(|tag| {
                let id = tag.id.clone();
                let summary = if with_counts {
                    TagSummary { item_count: tag.item_count, ..tag }
                } else {
                    TagSummary { item_count: None, ..tag }
                };
                let nested = build(&Some(id), children, with_counts);
                TagOut {
                    summary,
                    children: nested,
                    created_at: ts_or_epoch(None),
                    updated_at: ts_or_epoch(None),
                }
            })
            .collect()
    }
    build(&None, &mut children, with_counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn mem_conn() -> r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager> {
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
        conn
    }

    #[test]
    fn type_crud_and_seed() {
        let conn = mem_conn();
        let (loc, item) = seed_defaults(&conn, "g1").unwrap();
        let types = list_types(&conn, "g1", None).unwrap();
        assert_eq!(types.len(), 2);
        let locations = list_types(&conn, "g1", Some(true)).unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].id, loc);
        assert!(find_type(&conn, "g1", &item).unwrap().is_some());
        assert!(find_type(&conn, "g1", "nope").unwrap().is_none());

        let req = EntityTypeCreate { name: "Tool".into(), description: String::new(), icon: String::new(), is_location: false };
        create_type(&conn, "g1", "t1", &req).unwrap();
        // duplicate name rejected
        assert!(create_type(&conn, "g1", "t2", &req).is_err());
        // delete free type ok, referenced type blocked
        delete_type(&conn, "g1", "t1").unwrap();
        conn.execute(
            "INSERT INTO entities (id, group_id, name, entity_type_id, created_at, updated_at)
             VALUES ('e1', 'g1', '客厅', ?1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            rusqlite::params![loc],
        )
        .unwrap();
        assert!(delete_type(&conn, "g1", &loc).is_err());
    }

    #[test]
    fn tag_crud_and_tree() {
        let conn = mem_conn();
        let parent = TagCreate { name: "电子".into(), ..Default::default() };
        create_tag(&conn, "g1", "t1", &parent).unwrap();
        let child = TagCreate { name: "手机".into(), parent_id: Some("t1".into()), ..Default::default() };
        create_tag(&conn, "g1", "t2", &child).unwrap();
        assert!(create_tag(&conn, "g1", "t3", &parent).is_err(), "dup name");

        let flat = list_tags(&conn, "g1", false).unwrap();
        assert_eq!(flat.len(), 2);
        let tree = tag_tree(flat, false);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].children.len(), 1);

        update_tag(&conn, "g1", "t1", &TagUpdate { name: "电子2".into(), ..Default::default() }).unwrap();
        delete_tag(&conn, "g1", "t1").unwrap();
        assert!(find_tag(&conn, "g1", "t2").unwrap().unwrap().parent_id.is_none());
    }
}
