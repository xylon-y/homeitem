//! Argon2id password hashing.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::Argon2;

use crate::error::{AppError, AppResult};

/// Minimum accepted password length.
pub const MIN_PASSWORD_LEN: usize = 8;

/// Validate password strength before spending CPU on Argon2.
///
/// # Errors
/// `Validation` when the password is too short or blank.
pub fn validate_password(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::Validation(format!(
            "密码至少需要 {MIN_PASSWORD_LEN} 个字符"
        )));
    }
    if password.trim().is_empty() {
        return Err(AppError::Validation("密码不能为空".into()));
    }
    Ok(())
}

/// Hash a plaintext password with Argon2id (random salt, PHC string).
///
/// # Errors
/// Fails only when the underlying hash operation fails (never for bad input).
pub fn hash_password(password: &str) -> AppResult<String> {
    validate_password(password)?;
    // argon2 0.6 generates and embeds a random salt automatically
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| AppError::Internal(format!("hash password: {e}")))?
        .to_string();
    Ok(hash)
}

/// Verify a password against a stored hash.
///
/// Unknown/malformed hashes return `false` instead of erroring so that a
/// corrupted row cannot become an auth bypass or a 500.
pub fn verify_password(password: &str, stored: &str) -> bool {
    match PasswordHash::new(stored) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let h = hash_password("s3cret!!").unwrap();
        assert!(verify_password("s3cret!!", &h));
        assert!(!verify_password("wrong", &h));
        assert!(!verify_password("s3cret!!", "not-a-hash"));
        assert!(hash_password("short").is_err(), "weak passwords must be rejected");
    }

    #[test]
    fn salts_differ() {
        let a = hash_password("xxxxxxx1").unwrap();
        let b = hash_password("xxxxxxx1").unwrap();
        assert_ne!(a, b, "each hash must use a fresh salt");
    }
}
