//! Wire-format error body shared by backend and frontend.

use serde::{Deserialize, Serialize};

/// Unified API error payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub code: String,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub fields: std::collections::HashMap<String, String>,
}

impl ApiError {
    pub fn new(code: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
            fields: std::collections::HashMap::new(),
        }
    }

    /// Attach per-field validation messages.
    pub fn with_field(mut self, field: impl Into<String>, message: impl Into<String>) -> Self {
        self.fields.insert(field.into(), message.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_serializes_without_empty_fields() {
        let s = serde_json::to_string(&ApiError::new("NOT_FOUND", "nope")).unwrap();
        assert!(!s.contains("fields"), "{s}");
        let e = ApiError::new("VALIDATION", "bad").with_field("name", "required");
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("name"), "{s}");
    }
}
