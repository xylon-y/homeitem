//! Shared API contract between the Salvo backend and the Leptos frontend.
//!
//! Everything here is plain serde data: DTOs, query filters, error bodies and
//! constants. This crate must never depend on backend or frontend internals.

pub mod dto;
pub mod error;
pub mod query;

/// API path prefix served by the backend.
pub const API_PREFIX: &str = "/api/v1";

/// Default listen address when `HOMEITEM_LISTEN_ADDR` is unset.
pub const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:7745";

/// Application version reported by `GET /status`.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Roles a user can hold inside a group (mirrors upstream HomeBox).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupRole {
    User,
    Owner,
}

impl GroupRole {
    /// Whether this role may manage group membership, invitations and notifiers.
    pub fn can_manage_group(self) -> bool {
        matches!(self, Self::Owner)
    }
}

impl std::fmt::Display for GroupRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => f.write_str("user"),
            Self::Owner => f.write_str("owner"),
        }
    }
}

impl std::str::FromStr for GroupRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "owner" => Ok(Self::Owner),
            other => Err(format!("unknown group role: {other}")),
        }
    }
}

/// Scope carried inside an access token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenScope {
    /// Full API access.
    User,
    /// Read-only media access for `<img src>` URLs (7 days).
    Attachments,
}

impl std::fmt::Display for TokenScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => f.write_str("user"),
            Self::Attachments => f.write_str("attachments"),
        }
    }
}

impl std::str::FromStr for TokenScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "attachments" => Ok(Self::Attachments),
            other => Err(format!("unknown token scope: {other}")),
        }
    }
}
