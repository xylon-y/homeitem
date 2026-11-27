//! Groups (tenants), membership and invitation tokens.

use common::dto::{GroupOut, InvitationOut};
use common::GroupRole;
use rusqlite::{params, Connection};

use super::{query_all, query_optional, text, text_opt, ts_opt, ts_or_epoch};
use crate::error::{AppError, AppResult};

/// Insert a new group; returns its id.
pub fn insert(conn: &Connection, id: &str, name: &str, currency: &str) -> AppResult<()> {
    let now = super::now();
    conn.execute(
        "INSERT INTO groups (id, name, currency, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
        params![id, name, currency, now],
    )?;
    Ok(())
}

/// Load a group by id.
pub fn find(conn: &Connection, id: &str) -> AppResult<GroupOut> {
    let row = query_optional(
        conn,
        "SELECT id, name, currency, created_at, updated_at FROM groups WHERE id = ?1",
        &[&id],
        |r| {
            Ok::<_, rusqlite::Error>(GroupOut {
                id: text(r, 0)?,
                name: text(r, 1)?,
                currency: text(r, 2)?,
                created_at: ts_or_epoch(r.get::<_, Option<String>>(3)?.as_deref()),
                updated_at: ts_or_epoch(r.get::<_, Option<String>>(4)?.as_deref()),
            })
        },
    )?;
    row.ok_or_else(|| AppError::NotFound("group not found".into()))
}

/// Rename / change currency.
pub fn update(conn: &Connection, id: &str, name: &str, currency: &str) -> AppResult<()> {
    let now = super::now();
    conn.execute(
        "UPDATE groups SET name = ?1, currency = ?2, updated_at = ?3 WHERE id = ?4",
        params![name, currency, now, id],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// invitations
// ---------------------------------------------------------------------------

/// Create an invitation token; returns the raw token.
pub fn create_invitation(
    conn: &Connection,
    id: &str,
    group_id: &str,
    token: &str,
    role: GroupRole,
    email: Option<&str>,
    expires_at: Option<&str>,
) -> AppResult<()> {
    let now = super::now();
    conn.execute(
        "INSERT INTO group_invitations (id, group_id, token, role, email, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, group_id, token, role.to_string(), email, now, expires_at],
    )?;
    Ok(())
}

/// Look up a valid (unexpired) invitation by token.
pub fn find_invitation(
    conn: &Connection,
    token: &str,
) -> AppResult<Option<(String, String, GroupRole)>> {
    let now = super::now();
    let row = query_optional(
        conn,
        "SELECT id, group_id, role FROM group_invitations
          WHERE token = ?1 AND (expires_at IS NULL OR expires_at > ?2)",
        &[&token, &now],
        |r| {
            let role: String = r.get(2)?;
            Ok::<_, rusqlite::Error>((
                text(r, 0)?,
                text(r, 1)?,
                role.parse().unwrap_or(GroupRole::User),
            ))
        },
    )?;
    Ok(row)
}

/// List invitations for a group.
pub fn list_invitations(conn: &Connection, group_id: &str) -> AppResult<Vec<InvitationOut>> {
    query_all(
        conn,
        "SELECT id, token, role, email, created_at, expires_at
           FROM group_invitations WHERE group_id = ?1 ORDER BY created_at DESC",
        &[&group_id],
        |r| {
            let role: String = r.get(2)?;
            Ok::<_, rusqlite::Error>(InvitationOut {
                id: text(r, 0)?,
                token: text(r, 1)?,
                role: role.parse().unwrap_or(GroupRole::User),
                email: text_opt(r, 3)?,
                created_at: ts_or_epoch(r.get::<_, Option<String>>(4)?.as_deref()),
                expires_at: ts_opt(r.get::<_, Option<String>>(5)?.as_deref()),
            })
        },
    )
}

/// Delete one invitation.
pub fn delete_invitation(conn: &Connection, group_id: &str, id: &str) -> AppResult<bool> {
    let n = conn.execute(
        "DELETE FROM group_invitations WHERE id = ?1 AND group_id = ?2",
        params![id, group_id],
    )?;
    Ok(n > 0)
}

/// Consume an invitation (delete after successful registration).
/// Atomically consume an invitation. Returns `false` if it was already used,
/// so a single token cannot register multiple accounts under concurrency.
pub fn consume_invitation(conn: &Connection, id: &str) -> AppResult<bool> {
    let n = conn.execute("DELETE FROM group_invitations WHERE id = ?1", params![id])?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn mem_conn() -> r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager> {
        let manager = r2d2_sqlite::SqliteConnectionManager::memory();
        let pool = r2d2::Pool::builder().max_size(1).build(manager).unwrap();
        migrations::run(&pool).unwrap();
        pool.get().unwrap()
    }

    #[test]
    fn invitation_lifecycle() {
        let conn = mem_conn();
        insert(&conn, "g1", "Home", "CNY").unwrap();
        create_invitation(&conn, "i1", "g1", "tok123", GroupRole::User, Some("a@x.com"), None)
            .unwrap();
        let found = find_invitation(&conn, "tok123").unwrap().expect("found");
        assert_eq!(found.1, "g1");
        assert_eq!(found.2, GroupRole::User);
        assert!(find_invitation(&conn, "wrong").unwrap().is_none());
        let list = list_invitations(&conn, "g1").unwrap();
        assert_eq!(list.len(), 1);
        assert!(delete_invitation(&conn, "g1", "i1").unwrap());
        assert!(list_invitations(&conn, "g1").unwrap().is_empty());
    }

    #[test]
    fn expired_invitation_ignored() {
        let conn = mem_conn();
        insert(&conn, "g1", "Home", "CNY").unwrap();
        create_invitation(
            &conn,
            "i1",
            "g1",
            "tok",
            GroupRole::Owner,
            None,
            Some("2000-01-01T00:00:00Z"),
        )
        .unwrap();
        assert!(find_invitation(&conn, "tok").unwrap().is_none());
    }
}
