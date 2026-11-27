//! User records.


use common::dto::{Ts, UserOut};
use common::GroupRole;
use rusqlite::{params, Connection};

use super::{from_bool, query_optional, text, ts_or_epoch, ts_opt};
use crate::error::{AppError, AppResult};

/// A user row joined with its (default) group membership.
#[derive(Debug, Clone)]
pub struct UserRecord {
    pub id: String,
    pub name: String,
    pub email: String,
    pub password_hash: Option<String>,
    pub is_superuser: bool,
    pub activated_on: Option<Ts>,
    pub created_at: Ts,
    pub updated_at: Ts,
    pub group_id: String,
    pub group_name: String,
    pub role: GroupRole,
    pub token_version: i64,
}

impl UserRecord {
    /// Project to the wire DTO.
    pub fn to_out(&self) -> UserOut {
        UserOut {
            id: self.id.clone(),
            name: self.name.clone(),
            email: self.email.clone(),
            group_id: self.group_id.clone(),
            group_name: self.group_name.clone(),
            role: self.role,
            is_superuser: self.is_superuser,
            activated_on: self.activated_on,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

const SELECT_USER: &str = "
SELECT u.id, u.name, u.email, u.password_hash, u.is_superuser, u.activated_on,
       u.created_at, u.updated_at, u.token_version,
       g.id, g.name, ug.role
  FROM users u
  JOIN user_groups ug ON ug.user_id = u.id
  JOIN groups g ON g.id = ug.group_id
";

fn map_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<UserRecord> {
    // columns: 0..8 users.*, 9 group id, 10 group name, 11 membership role
    let role_raw: String = row.get(11)?;
    let role = role_raw.parse().unwrap_or(GroupRole::User);
    Ok(UserRecord {
        id: text(row, 0)?,
        name: text(row, 1)?,
        email: text(row, 2)?,
        password_hash: row.get(3)?,
        is_superuser: from_bool(row.get(4)?),
        activated_on: ts_opt(row.get::<_, Option<String>>(5)?.as_deref()),
        created_at: ts_or_epoch(row.get::<_, Option<String>>(6)?.as_deref()),
        updated_at: ts_or_epoch(row.get::<_, Option<String>>(7)?.as_deref()),
        token_version: row.get::<_, i64>(8).unwrap_or(0),
        group_id: text(row, 9)?,
        group_name: text(row, 10)?,
        role,
    })
}

/// Is any user registered yet? (drives `GET /status` → needsSetup)
pub fn any_user_exists(conn: &Connection) -> AppResult<bool> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
    Ok(n > 0)
}

/// Find a user by email (case-insensitive).
pub fn find_by_email(conn: &Connection, email: &str) -> AppResult<Option<UserRecord>> {
    let sql = format!("{SELECT_USER} WHERE u.email = ?1 COLLATE NOCASE");
    query_optional(conn, &sql, &[&email], map_user)
}

/// Find a user by email or display name (case-insensitive) for login.
pub fn find_by_login(conn: &Connection, login: &str) -> AppResult<Option<UserRecord>> {
    let sql = format!(
        "{SELECT_USER} WHERE u.email = ?1 COLLATE NOCASE OR u.name = ?1 COLLATE NOCASE LIMIT 1"
    );
    query_optional(conn, &sql, &[&login], map_user)
}

/// Find a user by id.
pub fn find_by_id(conn: &Connection, id: &str) -> AppResult<Option<UserRecord>> {
    let sql = format!("{SELECT_USER} WHERE u.id = ?1");
    query_optional(conn, &sql, &[&id], map_user)
}

/// All members of a group.
pub fn list_group_members(conn: &Connection, group_id: &str) -> AppResult<Vec<UserRecord>> {
    let sql = format!("{SELECT_USER} WHERE ug.group_id = ?1 ORDER BY u.created_at");
    super::query_all(conn, &sql, &[&group_id], map_user)
}

/// Insert a user and bind it to a group with the given role.
pub fn insert(
    conn: &Connection,
    id: &str,
    name: &str,
    email: &str,
    password_hash: Option<&str>,
    group_id: &str,
    role: GroupRole,
    activated: bool,
) -> AppResult<()> {
    let now = super::now();
    conn.execute(
        "INSERT INTO users (id, name, email, password_hash, is_superuser, activated_on, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?6)",
        params![id, name, email, password_hash, if activated { Some(now.as_str()) } else { None }, now],
    )?;
    conn.execute(
        "INSERT INTO user_groups (user_id, group_id, role) VALUES (?1, ?2, ?3)",
        params![id, group_id, role.to_string()],
    )?;
    Ok(())
}

/// Update profile fields (only the provided ones).
pub fn update_profile(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    email: Option<&str>,
) -> AppResult<()> {
    let now = super::now();
    if let Some(name) = name {
        conn.execute(
            "UPDATE users SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, now, id],
        )?;
    }
    if let Some(email) = email {
        conn.execute(
            "UPDATE users SET email = ?1, updated_at = ?2 WHERE id = ?3",
            params![email, now, id],
        )?;
    }
    Ok(())
}

/// Replace the stored password hash and bump the token version so every
/// previously issued JWT is invalidated immediately.
pub fn set_password(conn: &Connection, id: &str, hash: &str) -> AppResult<()> {
    let now = super::now();
    conn.execute(
        "UPDATE users SET password_hash = ?1, updated_at = ?2, token_version = token_version + 1 WHERE id = ?3",
        params![hash, now, id],
    )?;
    Ok(())
}

/// The current token version of a user (0 when not found).
pub fn token_version(conn: &Connection, id: &str) -> AppResult<i64> {
    Ok(conn
        .query_row("SELECT token_version FROM users WHERE id = ?1", params![id], |r| r.get(0))
        .unwrap_or(0))
}

/// Does this email already belong to another account?
pub fn email_taken(conn: &Connection, email: &str, except_id: Option<&str>) -> AppResult<bool> {
    let n: i64 = match except_id {
        Some(id) => conn.query_row(
            "SELECT COUNT(*) FROM users WHERE email = ?1 COLLATE NOCASE AND id <> ?2",
            params![email, id],
            |r| r.get(0),
        )?,
        None => conn.query_row(
            "SELECT COUNT(*) FROM users WHERE email = ?1 COLLATE NOCASE",
            params![email],
            |r| r.get(0),
        )?,
    };
    Ok(n > 0)
}

/// Mark the account activated (used after first login/registration).
pub fn activate(conn: &Connection, id: &str) -> AppResult<()> {
    let now = super::now();
    conn.execute(
        "UPDATE users SET activated_on = COALESCE(activated_on, ?1), updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    Ok(())
}

/// Count users in a group (for statistics).
pub fn count_group_users(conn: &Connection, group_id: &str) -> AppResult<i64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM user_groups WHERE group_id = ?1",
        params![group_id],
        |r| r.get(0),
    )?;
    Ok(n)
}

/// Set the user's role inside a group.
pub fn set_role(conn: &Connection, user_id: &str, group_id: &str, role: GroupRole) -> AppResult<()> {
    conn.execute(
        "UPDATE user_groups SET role = ?1 WHERE user_id = ?2 AND group_id = ?3",
        params![role.to_string(), user_id, group_id],
    )?;
    Ok(())
}

/// Convenience for tests: does the user belong to the group?
pub fn in_group(conn: &Connection, user_id: &str, group_id: &str) -> AppResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM user_groups WHERE user_id = ?1 AND group_id = ?2",
        params![user_id, group_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Helper used by setup: create group + owner in one go.
pub fn count_users(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?)
}

/// First-run setup: create the owner user + its default group, activate.
///
/// Returns the joined record for token issuance.
pub fn setup_owner(conn: &Connection, name: &str, email: &str, hash: &str) -> AppResult<UserRecord> {
    // Re-check inside the write transaction: the handler's read-side check is
    // racy, and two concurrent /setup calls must not both create an owner.
    if count_users(conn)? > 0 {
        return Err(AppError::Conflict("系统已初始化，请直接登录".into()));
    }
    let user_id = uuid::Uuid::new_v4().to_string();
    let group_id = uuid::Uuid::new_v4().to_string();
    super::groups::insert(conn, &group_id, if name.trim().is_empty() { "我的家庭" } else { name }, "CNY")?;
    // seed default Location/Item types so the group is usable immediately
    super::taxonomy::seed_defaults(conn, &group_id)?;
    insert(conn, &user_id, name, email, Some(hash), &group_id, GroupRole::Owner, true)?;
    // owner is a superuser for first-run convenience
    conn.execute("UPDATE users SET is_superuser = 1 WHERE id = ?1", params![user_id])?;
    find_by_id(conn, &user_id)?.ok_or_else(|| AppError::Internal("setup owner vanished".into()))
}

/// Registration through an invitation token.
///
/// `open_registration` additionally allows sign-up without a token.
pub fn register_with_invitation(
    conn: &Connection,
    open_registration: bool,
    name: &str,
    email: &str,
    hash: &str,
    token: &str,
) -> AppResult<UserRecord> {
    let user_id = uuid::Uuid::new_v4().to_string();
    let created = if token.trim().is_empty() {
        if !open_registration {
            return Err(AppError::Forbidden("当前未开放注册，需要邀请令牌".into()));
        }
        // no group: registration without invitation only joins an existing
        // group when open_registration is enabled — put them in their own group
        let group_id = uuid::Uuid::new_v4().to_string();
        super::groups::insert(conn, &group_id, "我的家庭", "CNY")?;
        super::taxonomy::seed_defaults(conn, &group_id)?;
        insert(conn, &user_id, name, email, Some(hash), &group_id, GroupRole::Owner, true)?;
        group_id
    } else {
        let (inv_id, inv_group, inv_role) = super::groups::find_invitation(conn, token)?
            .ok_or_else(|| AppError::BadRequest("邀请令牌无效或已过期".into()))?;
        // Consume atomically before inserting so a concurrent registration
        // cannot reuse the same invitation token.
        if !super::groups::consume_invitation(conn, &inv_id)? {
            return Err(AppError::BadRequest("邀请令牌无效或已过期".into()));
        }
        insert(conn, &user_id, name, email, Some(hash), &inv_group, inv_role, true)?;
        inv_group
    };
    let _ = created;
    find_by_id(conn, &user_id)?.ok_or_else(|| AppError::Internal("registered user vanished".into()))
}

/// Remove a user from a group (the user row itself is kept).
pub fn remove_member(conn: &Connection, group_id: &str, user_id: &str) -> AppResult<()> {
    let n = conn.execute(
        "DELETE FROM user_groups WHERE user_id = ?1 AND group_id = ?2",
        params![user_id, group_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("成员不存在".into()));
    }
    Ok(())
}

/// How many owners the group currently has.
pub fn count_owners(conn: &Connection, group_id: &str) -> AppResult<i64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM user_groups WHERE group_id = ?1 AND role = 'owner'",
        params![group_id],
        |r| r.get(0),
    )?;
    Ok(n)
}

/// The user's role in a group, if a member.
pub fn member_role(conn: &Connection, group_id: &str, user_id: &str) -> AppResult<Option<GroupRole>> {
    let raw: Option<String> = super::query_optional(
        conn,
        "SELECT role FROM user_groups WHERE group_id = ?1 AND user_id = ?2",
        &[&group_id, &user_id],
        |r| super::text(r, 0),
    )?;
    Ok(raw.and_then(|s| s.parse().ok()))
}

/// Members projected to the wire DTO.
pub fn list_members_out(conn: &Connection, group_id: &str) -> AppResult<Vec<common::dto::MemberOut>> {
    Ok(list_group_members(conn, group_id)?
        .into_iter()
        .map(|u| common::dto::MemberOut {
            user_id: u.id,
            name: u.name,
            email: u.email,
            role: u.role,
            activated_on: u.activated_on,
        })
        .collect())
}

/// Unused-import guard: keeps `bool_sql` referenced for future flags.
#[allow(dead_code)]
fn _keep(b: bool) -> i64 {
    super::bool_sql(b)
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
    fn setup_and_lookup() {
        let conn = mem_conn();
        assert!(!any_user_exists(&conn).unwrap());
        conn.execute(
            "INSERT INTO groups (id, name, currency, created_at, updated_at)
             VALUES ('g1', 'Home', 'CNY', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        insert(&conn, "u1", "Alice", "a@x.com", Some("hash"), "g1", GroupRole::Owner, true).unwrap();
        assert!(any_user_exists(&conn).unwrap());
        let u = find_by_email(&conn, "A@X.COM").unwrap().expect("found");
        assert_eq!(u.name, "Alice");
        assert_eq!(u.role, GroupRole::Owner);
        assert_eq!(u.group_name, "Home");
        assert!(u.activated_on.is_some());
        assert!(find_by_email(&conn, "nobody@x.com").unwrap().is_none());
    }

    #[test]
    fn member_listing() {
        let conn = mem_conn();
        conn.execute(
            "INSERT INTO groups (id, name, currency, created_at, updated_at)
             VALUES ('g1', 'Home', 'CNY', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        insert(&conn, "u1", "A", "a@x.com", None, "g1", GroupRole::Owner, true).unwrap();
        insert(&conn, "u2", "B", "b@x.com", None, "g1", GroupRole::User, false).unwrap();
        let members = list_group_members(&conn, "g1").unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(count_group_users(&conn, "g1").unwrap(), 2);
    }

    #[test]
    fn setup_owner_is_idempotent_guard() {
        let conn = mem_conn();
        setup_owner(&conn, "Owner", "o@x.com", "hash").unwrap();
        // A second setup attempt inside the same write path must be rejected.
        assert!(setup_owner(&conn, "Owner2", "o2@x.com", "hash").is_err());
        assert_eq!(count_users(&conn).unwrap(), 1);
    }

    #[test]
    fn invitation_consumption_is_atomic() {
        let conn = mem_conn();
        conn.execute(
            "INSERT INTO groups (id, name, currency, created_at, updated_at)
             VALUES ('g1', 'Home', 'CNY', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        super::super::groups::create_invitation(&conn, "inv1", "g1", "tok123", GroupRole::User, None, Some("2027-01-02T00:00:00Z")).unwrap();
        let (id, gid, role) = super::super::groups::find_invitation(&conn, "tok123").unwrap().unwrap();
        assert_eq!(gid, "g1");
        assert!(matches!(role, GroupRole::User));
        assert!(super::super::groups::consume_invitation(&conn, &id).unwrap());
        assert!(!super::super::groups::consume_invitation(&conn, &id).unwrap());
        assert!(super::super::groups::find_invitation(&conn, "tok123").unwrap().is_none());
    }
}
