//! Schema migrations, applied in order and tracked with `PRAGMA user_version`.
//!
//! Rules: append-only — never edit an already-applied migration.

use r2d2_sqlite::SqliteConnectionManager;


use crate::error::{AppError, AppResult};

/// Ordered migrations. Each entry runs inside its own transaction.
static MIGRATIONS: &[(&str, &str)] = &[( "001_init", r#"
CREATE TABLE app_settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE users (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  email         TEXT NOT NULL COLLATE NOCASE,
  password_hash TEXT,
  is_superuser  INTEGER NOT NULL DEFAULT 0,
  activated_on  TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_users_email ON users(email COLLATE NOCASE);

CREATE TABLE groups (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  currency   TEXT NOT NULL DEFAULT 'CNY',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE user_groups (
  user_id  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
  role     TEXT NOT NULL DEFAULT 'user',
  PRIMARY KEY (user_id, group_id)
);
CREATE INDEX idx_user_groups_group ON user_groups(group_id);

CREATE TABLE group_invitations (
  id         TEXT PRIMARY KEY,
  group_id   TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
  token      TEXT NOT NULL UNIQUE,
  role       TEXT NOT NULL DEFAULT 'user',
  email      TEXT,
  created_at TEXT NOT NULL,
  expires_at TEXT
);
CREATE INDEX idx_invitations_group ON group_invitations(group_id);

CREATE TABLE entity_types (
  id          TEXT PRIMARY KEY,
  group_id    TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  icon        TEXT NOT NULL DEFAULT '',
  is_location INTEGER NOT NULL DEFAULT 0,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_entity_types_group_name
  ON entity_types(group_id, name COLLATE NOCASE);

CREATE TABLE entities (
  id                          TEXT PRIMARY KEY,
  group_id                    TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
  name                        TEXT NOT NULL,
  description                 TEXT NOT NULL DEFAULT '',
  quantity                    REAL NOT NULL DEFAULT 1,
  min_quantity                REAL NOT NULL DEFAULT 0,
  asset_id                    INTEGER NOT NULL DEFAULT 0,
  archived                    INTEGER NOT NULL DEFAULT 0,
  insured                     INTEGER NOT NULL DEFAULT 0,
  notes                       TEXT NOT NULL DEFAULT '',
  parent_id                   TEXT REFERENCES entities(id) ON DELETE CASCADE,
  location_id                 TEXT REFERENCES entities(id) ON DELETE SET NULL,
  sync_child_entity_locations INTEGER NOT NULL DEFAULT 0,
  entity_type_id              TEXT NOT NULL REFERENCES entity_types(id) ON DELETE RESTRICT,
  serial_number               TEXT NOT NULL DEFAULT '',
  model_number                TEXT NOT NULL DEFAULT '',
  manufacturer                TEXT NOT NULL DEFAULT '',
  lifetime_warranty           INTEGER NOT NULL DEFAULT 0,
  warranty_expires            TEXT,
  warranty_details            TEXT NOT NULL DEFAULT '',
  purchase_date               TEXT,
  purchase_from               TEXT NOT NULL DEFAULT '',
  purchase_price              REAL NOT NULL DEFAULT 0,
  sold_date                   TEXT,
  sold_to                     TEXT NOT NULL DEFAULT '',
  sold_price                  REAL NOT NULL DEFAULT 0,
  sold_notes                  TEXT NOT NULL DEFAULT '',
  reminder_date               TEXT,
  reminder_note               TEXT NOT NULL DEFAULT '',
  import_ref                  TEXT NOT NULL DEFAULT '',
  created_at                  TEXT NOT NULL,
  updated_at                  TEXT NOT NULL
);
CREATE INDEX idx_entities_name       ON entities(group_id, name);
CREATE INDEX idx_entities_parent     ON entities(parent_id);
CREATE INDEX idx_entities_type_arch  ON entities(group_id, entity_type_id, archived);
CREATE INDEX idx_entities_asset      ON entities(group_id, asset_id);
CREATE INDEX idx_entities_warranty   ON entities(warranty_expires);
CREATE INDEX idx_entities_reminder   ON entities(reminder_date);
CREATE INDEX idx_entities_purchased  ON entities(purchase_date);

CREATE TABLE tags (
  id          TEXT PRIMARY KEY,
  group_id    TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  color       TEXT NOT NULL DEFAULT '',
  icon        TEXT NOT NULL DEFAULT '',
  parent_id   TEXT REFERENCES tags(id) ON DELETE SET NULL,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_tags_group_name ON tags(group_id, name COLLATE NOCASE);

CREATE TABLE entity_tags (
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  tag_id    TEXT NOT NULL REFERENCES tags(id)     ON DELETE CASCADE,
  PRIMARY KEY (entity_id, tag_id)
);
CREATE INDEX idx_entity_tags_tag ON entity_tags(tag_id);

CREATE TABLE entity_fields (
  id            TEXT PRIMARY KEY,
  entity_id     TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  type          TEXT NOT NULL DEFAULT 'text',
  text_value    TEXT NOT NULL DEFAULT '',
  number_value  REAL NOT NULL DEFAULT 0,
  boolean_value INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_entity_fields_entity ON entity_fields(entity_id);
CREATE INDEX idx_entity_fields_name   ON entity_fields(name);

CREATE TABLE attachments (
  id            TEXT PRIMARY KEY,
  entity_id     TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  type          TEXT NOT NULL DEFAULT 'attachment',
  title         TEXT NOT NULL DEFAULT '',
  file_name     TEXT NOT NULL DEFAULT '',
  path          TEXT NOT NULL,
  mime_type     TEXT NOT NULL DEFAULT 'application/octet-stream',
  size_bytes    INTEGER NOT NULL DEFAULT 0,
  primary_photo INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);
CREATE INDEX idx_attachments_entity ON attachments(entity_id);

CREATE TABLE maintenance_entries (
  id             TEXT PRIMARY KEY,
  entity_id      TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  name           TEXT NOT NULL,
  description    TEXT NOT NULL DEFAULT '',
  cost           REAL NOT NULL DEFAULT 0,
  completed_date TEXT,
  scheduled_date TEXT,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL
);
CREATE INDEX idx_maint_entity    ON maintenance_entries(entity_id);
CREATE INDEX idx_maint_scheduled ON maintenance_entries(scheduled_date);

CREATE TABLE notifiers (
  id         TEXT PRIMARY KEY,
  group_id   TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
  name       TEXT NOT NULL,
  kind       TEXT NOT NULL DEFAULT 'email',
  target     TEXT NOT NULL,
  is_active  INTEGER NOT NULL DEFAULT 1,
  kinds      TEXT NOT NULL DEFAULT 'warranty,reminder,maintenance,lowStock',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_notifiers_group ON notifiers(group_id, is_active);

CREATE TABLE notification_log (
  id           TEXT PRIMARY KEY,
  notifier_id  TEXT NOT NULL REFERENCES notifiers(id) ON DELETE CASCADE,
  dedupe_key   TEXT NOT NULL,
  sent_at      TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_notification_dedupe ON notification_log(notifier_id, dedupe_key);

-- seed: default entity types per group are created by the setup handler
"#), ("002_auth_hardening", r#"
-- Token version lets password changes revoke all previously issued tokens.
ALTER TABLE users ADD COLUMN token_version INTEGER NOT NULL DEFAULT 0;

-- Fix lowstock spelling for rows created by older builds (enum serializes as
-- lowercase "lowstock"); map_notifier already tolerates both spellings.
UPDATE notifiers SET kinds = REPLACE(kinds, 'lowStock', 'lowstock') WHERE kinds LIKE '%lowStock%';

-- Faster login lookup by display name in addition to the unique email index.
CREATE INDEX idx_users_name ON users(name COLLATE NOCASE);
"#)];

/// Apply all pending migrations.
///
/// # Errors
/// Fails when a migration statement is rejected by SQLite.
pub fn run(pool: &r2d2::Pool<SqliteConnectionManager>) -> AppResult<()> {
    let mut conn = pool.get()?;
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let applied = current as usize;
    if applied > MIGRATIONS.len() {
        return Err(AppError::Internal(format!(
            "database schema version {applied} is newer than this build supports ({})",
            MIGRATIONS.len()
        )));
    }
    for (idx, (name, sql)) in MIGRATIONS.iter().enumerate().skip(applied) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)
            .map_err(|e| AppError::Internal(format!("migration {name} failed: {e}")))?;
        tx.pragma_update(None, "user_version", (idx + 1) as i64)?;
        tx.commit()?;
        tracing::info!(migration = name, "applied");
    }
    Ok(())
}

/// Number of migrations compiled in (for tests/status).
pub fn count() -> usize {
    MIGRATIONS.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_have_unique_names() {
        let mut seen = std::collections::HashSet::new();
        for (name, _) in MIGRATIONS {
            assert!(seen.insert(*name), "duplicate migration {name}");
        }
    }

    #[test]
    fn applies_to_fresh_db() {
        let manager = SqliteConnectionManager::memory();
        let pool = r2d2::Pool::builder().max_size(1).build(manager).unwrap();
        run(&pool).unwrap();
        let conn = pool.get().unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
        // idempotent
        drop(conn);
        run(&pool).unwrap();
    }
}
