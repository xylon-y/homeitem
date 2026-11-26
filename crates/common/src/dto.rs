//! JSON DTOs for every REST endpoint (camelCase on the wire).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::GroupRole;

/// ISO-8601 UTC timestamp used across the API.
pub type Ts = DateTime<Utc>;

// ---------------------------------------------------------------------------
// pagination / envelope
// ---------------------------------------------------------------------------

/// Standard paginated list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: i64,
    pub page_size: i64,
    pub total: i64,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, page: i64, page_size: i64, total: i64) -> Self {
        Self { items, page, page_size, total }
    }
}

// ---------------------------------------------------------------------------
// system
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub version: String,
    pub build: String,
    pub needs_setup: bool,
    pub registration_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Currency {
    pub code: String,
    pub symbol: String,
    pub label: String,
}

// ---------------------------------------------------------------------------
// auth / users
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub stay_logged_in: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    pub name: String,
    pub email: String,
    pub password: String,
    /// Invitation token from a group owner.
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenResponse {
    pub token: String,
    pub attachment_token: String,
    pub expires_at: Ts,
    pub user: UserOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOut {
    pub id: String,
    pub name: String,
    pub email: String,
    pub group_id: String,
    pub group_name: String,
    pub role: GroupRole,
    pub is_superuser: bool,
    pub activated_on: Option<Ts>,
    pub created_at: Ts,
    pub updated_at: Ts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserUpdate {
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordRequest {
    pub current: String,
    pub new: String,
}

// ---------------------------------------------------------------------------
// groups
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupOut {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub created_at: Ts,
    pub updated_at: Ts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupUpdate {
    pub name: String,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberOut {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub role: GroupRole,
    pub activated_on: Option<Ts>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationOut {
    pub id: String,
    pub token: String,
    pub role: GroupRole,
    pub email: Option<String>,
    pub created_at: Ts,
    pub expires_at: Option<Ts>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationCreate {
    pub role: GroupRole,
    pub email: Option<String>,
}

// ---------------------------------------------------------------------------
// entity types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityTypeOut {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub is_location: bool,
    pub created_at: Ts,
    pub updated_at: Ts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityTypeCreate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub is_location: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityTypeUpdate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub is_location: bool,
}

// ---------------------------------------------------------------------------
// tags
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub color: String,
    pub icon: String,
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagOut {
    #[serde(flatten)]
    pub summary: TagSummary,
    pub children: Vec<TagOut>,
    pub created_at: Ts,
    pub updated_at: Ts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCreate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub icon: String,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagUpdate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub icon: String,
    pub parent_id: Option<String>,
}

// ---------------------------------------------------------------------------
// entities (items + locations share one table, like upstream)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityFieldData {
    pub id: String,
    pub name: String,
    /// "text" | "number" | "boolean"
    #[serde(rename = "type")]
    pub kind: String,
    pub text_value: String,
    pub number_value: f64,
    pub boolean_value: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemAttachment {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub file_name: String,
    pub path: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub primary: bool,
    pub created_at: Ts,
    pub updated_at: Ts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntitySummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub quantity: f64,
    pub min_quantity: f64,
    pub asset_id: i64,
    pub archived: bool,
    pub insured: bool,
    pub entity_type: Option<EntityTypeOut>,
    pub parent: Option<Box<EntitySummary>>,
    /// Resolved location (own override or nearest location ancestor).
    pub location: Option<Box<EntitySummary>>,
    pub location_id: Option<String>,
    pub tags: Vec<TagSummary>,
    pub thumbnail_id: Option<String>,
    pub purchase_price: f64,
    pub total_price: f64,
    pub warranty_expires: Option<Ts>,
    pub reminder_date: Option<Ts>,
    pub sold_date: Option<Ts>,
    pub sold_price: f64,
    pub item_count: i64,
    pub created_at: Ts,
    pub updated_at: Ts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityOut {
    #[serde(flatten)]
    pub summary: EntitySummary,
    pub serial_number: String,
    pub model_number: String,
    pub manufacturer: String,
    pub notes: String,
    pub lifetime_warranty: bool,
    pub warranty_details: String,
    pub purchase_date: Option<Ts>,
    pub purchase_from: String,
    pub sold_to: String,
    pub sold_notes: String,
    pub reminder_note: String,
    pub sync_child_entity_locations: bool,
    pub fields: Vec<EntityFieldData>,
    pub attachments: Vec<ItemAttachment>,
    pub maintenance_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityCreate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub entity_type_id: String,
    pub parent_id: Option<String>,
    pub location_id: Option<String>,
    #[serde(default = "default_quantity")]
    pub quantity: f64,
    #[serde(default)]
    pub min_quantity: f64,
    #[serde(default)]
    pub tag_ids: Vec<String>,
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub model_number: String,
    #[serde(default)]
    pub serial_number: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub purchase_price: f64,
    #[serde(default)]
    pub purchase_date: Option<Ts>,
    #[serde(default)]
    pub purchase_from: String,
    #[serde(default)]
    pub warranty_expires: Option<Ts>,
}

fn default_quantity() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityUpdate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub entity_type_id: String,
    pub parent_id: Option<String>,
    pub location_id: Option<String>,
    pub quantity: f64,
    #[serde(default)]
    pub min_quantity: f64,
    #[serde(default)]
    pub tag_ids: Vec<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub insured: bool,
    #[serde(default)]
    pub asset_id: i64,
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub model_number: String,
    #[serde(default)]
    pub serial_number: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub lifetime_warranty: bool,
    pub warranty_expires: Option<Ts>,
    #[serde(default)]
    pub warranty_details: String,
    pub purchase_date: Option<Ts>,
    #[serde(default)]
    pub purchase_from: String,
    pub purchase_price: f64,
    pub sold_date: Option<Ts>,
    #[serde(default)]
    pub sold_to: String,
    pub sold_price: f64,
    #[serde(default)]
    pub sold_notes: String,
    pub reminder_date: Option<Ts>,
    #[serde(default)]
    pub reminder_note: String,
    #[serde(default)]
    pub sync_child_entity_locations: bool,
    #[serde(default)]
    pub fields: Vec<EntityFieldData>,
}

/// Partial update (mirrors upstream EntityPatch).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_type_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateOptions {
    #[serde(default)]
    pub copy_maintenance: bool,
    #[serde(default)]
    pub copy_attachments: bool,
    #[serde(default)]
    pub copy_custom_fields: bool,
    #[serde(default)]
    pub copy_prefix: String,
}

/// One step of the breadcrumb returned by `GET /entities/{id}/path`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityPath {
    pub id: String,
    pub name: String,
}

/// Node returned by `GET /entities/tree`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeItem {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub is_location: bool,
    pub item_count: i64,
    pub children: Vec<TreeItem>,
}

// ---------------------------------------------------------------------------
// maintenance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceEntry {
    pub id: String,
    pub entity_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub cost: f64,
    pub completed_date: Option<Ts>,
    pub scheduled_date: Option<Ts>,
    pub created_at: Ts,
    pub updated_at: Ts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceCreate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub cost: f64,
    pub completed_date: Option<Ts>,
    pub scheduled_date: Option<Ts>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceUpdate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub cost: f64,
    pub completed_date: Option<Ts>,
    pub scheduled_date: Option<Ts>,
}

// ---------------------------------------------------------------------------
// reminders
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReminderKind {
    Warranty,
    Reminder,
    Maintenance,
    LowStock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderItem {
    pub kind: ReminderKind,
    pub entity_id: String,
    pub entity_name: String,
    pub date: Ts,
    pub days_remaining: i64,
    pub overdue: bool,
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderCounts {
    pub overdue: i64,
    pub due_soon: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderResponse {
    pub items: Vec<ReminderItem>,
    pub counts: ReminderCounts,
}

// ---------------------------------------------------------------------------
// statistics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Statistics {
    pub total_items: i64,
    pub total_value: f64,
    pub total_locations: i64,
    pub total_tags: i64,
    pub total_users: i64,
    pub total_with_warranty: i64,
    pub total_archived: i64,
    pub total_low_stock: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TotalsByOrganizer {
    pub id: String,
    pub name: String,
    pub total: f64,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueOverTimeEntry {
    pub date: Ts,
    pub name: String,
    pub value: f64,
}

// ---------------------------------------------------------------------------
// notifiers (email push, following upstream notifier concept)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifierKind {
    Email,
    Webhook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifierOut {
    pub id: String,
    pub name: String,
    pub kind: NotifierKind,
    /// Email address (kind=email) or URL (kind=webhook).
    pub target: String,
    pub is_active: bool,
    /// Reminder kinds this notifier subscribes to.
    pub kinds: Vec<ReminderKind>,
    pub created_at: Ts,
    pub updated_at: Ts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifierCreate {
    pub name: String,
    pub kind: NotifierKind,
    pub target: String,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub kinds: Vec<ReminderKind>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifierUpdate {
    pub name: String,
    pub kind: NotifierKind,
    pub target: String,
    pub is_active: bool,
    pub kinds: Vec<ReminderKind>,
}

/// Request body for `POST /notifiers/{id}/test`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifierTest {
    #[serde(default)]
    pub message: String,
}

// ---------------------------------------------------------------------------
// attachments upload metadata
// ---------------------------------------------------------------------------

/// Form fields accepted alongside the `file` part of an upload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentForm {
    pub name: String,
    #[serde(rename = "type", default = "default_attachment_type")]
    pub kind: String,
    #[serde(default)]
    pub primary: bool,
}

fn default_attachment_type() -> String {
    "attachment".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_create_defaults_quantity() {
        let v: EntityCreate = serde_json::from_str(
            r#"{"name":"x","entityTypeId":"t"}"#,
        )
        .unwrap_or_else(|_| serde_json::from_str(
            r#"{"name":"x","entityTypeId":"t"}"#,
        ).unwrap());
        assert!((v.quantity - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn wire_names_are_camel_case() {
        let s = serde_json::to_string(&TagSummary {
            id: "1".into(),
            name: "n".into(),
            description: String::new(),
            color: String::new(),
            icon: String::new(),
            parent_id: Some("p".into()),
            item_count: None,
        })
        .unwrap();
        assert!(s.contains("parentId"), "{s}");
        assert!(s.contains("itemId") || !s.contains("itemCount"), "{s}");
    }
}
