//! Query-string filter contract for `GET /entities` and friends.

use serde::{Deserialize, Serialize};

/// Filters for `GET /api/v1/entities`.
///
/// Array-valued params accept either repeated keys or a single comma-separated
/// value (e.g. `?tags=a,b`).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EntitiesQuery {
    /// Free-text search across name/description/serial/model/manufacturer/notes/asset id.
    pub q: String,
    pub page: i64,
    pub page_size: i64,
    /// name | createdAt | updatedAt | purchasePrice | warrantyExpires | assetId
    pub order_by: String,
    /// asc | desc
    pub order_direction: String,
    pub include_archived: bool,
    /// Comma-separated entity ids whose children are returned.
    pub parent_ids: Option<String>,
    pub entity_type_ids: Option<String>,
    /// Comma-separated tag ids.
    pub tags: Option<String>,
    pub negate_tags: bool,
    pub match_all_tags: bool,
    /// Only entities whose type is a location.
    pub is_location: Option<bool>,
    pub only_with_photo: bool,
    pub only_without_photo: bool,
    /// Only entities with quantity <= min_quantity and min_quantity > 0.
    pub low_stock: bool,
    /// Custom field filters, reserved: `name:value` pairs, comma separated.
    pub fields: Option<String>,
}

impl EntitiesQuery {
    /// Effective page number (1-based).
    pub fn page(&self) -> i64 {
        if self.page < 1 { 1 } else { self.page }
    }

    /// Effective page size, clamped to 1..=100 (default 25).
    pub fn page_size(&self) -> i64 {
        match self.page_size {
            0 => 25,
            n if n < 1 => 1,
            n if n > 100 => 100,
            n => n,
        }
    }

    /// Resolve the ORDER BY column against a whitelist.
    pub fn order_column(&self) -> &'static str {
        match self.order_by.as_str() {
            "createdAt" => "created_at",
            "updatedAt" => "updated_at",
            "purchasePrice" => "purchase_price",
            "warrantyExpires" => "warranty_expires",
            "assetId" => "asset_id",
            "quantity" => "quantity",
            _ => "name",
        }
    }

    /// Resolve ASC/DESC, defaulting to ascending.
    pub fn order_dir(&self) -> &'static str {
        if self.order_direction.eq_ignore_ascii_case("desc") { "DESC" } else { "ASC" }
    }

    /// Split an optional comma-separated id list.
    pub fn id_list(raw: &Option<String>) -> Vec<String> {
        raw.as_deref()
            .map(|s| s.split(',').map(str::trim).filter(|p| !p.is_empty()).map(ToString::to_string).collect())
            .unwrap_or_default()
    }
}

/// Query for `GET /reminders`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RemindersQuery {
    /// Days ahead to include (default 30).
    pub within: i64,
    pub include_overdue: bool,
    pub include_done: bool,
    /// Comma separated: warranty,reminder,maintenance,lowStock (default all).
    pub kinds: Option<String>,
}

impl Default for RemindersQuery {
    fn default() -> Self {
        Self { within: 30, include_overdue: true, include_done: false, kinds: None }
    }
}

impl RemindersQuery {
    /// Which reminder kinds to include (all when `kinds` is absent).
    pub fn kinds(&self) -> Vec<String> {
        match self.kinds.as_deref() {
            Some(raw) if !raw.trim().is_empty() => raw
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(ToString::to_string)
                .collect(),
            _ => vec!["warranty".into(), "reminder".into(), "maintenance".into(), "lowStock".into()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let q = EntitiesQuery::default();
        assert_eq!(q.page(), 1);
        assert_eq!(q.page_size(), 25);
        assert_eq!(q.order_column(), "name");
        assert_eq!(q.order_dir(), "ASC");
    }

    #[test]
    fn page_size_clamps() {
        let mut q = EntitiesQuery { page_size: 5000, ..Default::default() };
        assert_eq!(q.page_size(), 100);
        q.page_size = -3;
        assert_eq!(q.page_size(), 1);
        q.page_size = 0;
        assert_eq!(q.page_size(), 25);
    }

    #[test]
    fn id_list_splits_csv() {
        let raw = Some(" a , b,,c ".to_string());
        assert_eq!(EntitiesQuery::id_list(&raw), vec!["a", "b", "c"]);
        assert!(EntitiesQuery::id_list(&None).is_empty());
    }

    #[test]
    fn deserialize_query_from_urlencoded() {
        // how Salvo hands us query params (serde_urlencoded compatible)
        let q: EntitiesQuery = serde_urlencoded_lite("q=tv&tags=a,b&pageSize=10&isLocation=true");
        assert_eq!(q.q, "tv");
        assert_eq!(q.tags.as_deref(), Some("a,b"));
        assert_eq!(q.page_size(), 10);
        assert_eq!(q.is_location, Some(true));
    }

    /// Minimal urlencoded deserializer for tests without extra deps.
    fn serde_urlencoded_lite(s: &str) -> EntitiesQuery {
        let mut out = EntitiesQuery::default();
        for kv in s.split('&') {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            let v = v.replace('+', " ");
            match k {
                "q" => out.q = v,
                "page" => out.page = v.parse().unwrap_or(0),
                "pageSize" => out.page_size = v.parse().unwrap_or(0),
                "orderBy" => out.order_by = v,
                "orderDirection" => out.order_direction = v,
                "includeArchived" => out.include_archived = v == "true",
                "parentIds" => out.parent_ids = Some(v),
                "entityTypeIds" => out.entity_type_ids = Some(v),
                "tags" => out.tags = Some(v),
                "negateTags" => out.negate_tags = v == "true",
                "matchAllTags" => out.match_all_tags = v == "true",
                "isLocation" => out.is_location = Some(v == "true"),
                "onlyWithPhoto" => out.only_with_photo = v == "true",
                "onlyWithoutPhoto" => out.only_without_photo = v == "true",
                "lowStock" => out.low_stock = v == "true",
                "fields" => out.fields = Some(v),
                _ => {}
            }
        }
        out
    }
}
