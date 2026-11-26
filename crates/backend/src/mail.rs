//! SMTP mail delivery for reminder notifications.
//!
//! Configuration comes from `HOMEITEM_SMTP_URL` (e.g.
//! `smtp://user:pass@mail.example.com:587` or `smtp://mail.example.com:25`).
//! When no SMTP URL is configured the sender is disabled and every send is a
//! no-op that reports `false`.

use lettre::message::header::ContentType;
#[cfg(test)]
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::Config;
use crate::error::{AppError, AppResult};

/// A configured mailer (absent when SMTP is not configured).
pub struct Mailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
}

impl Mailer {
    /// Build a mailer from configuration.
    ///
    /// # Errors
    /// Returns `Internal` when `HOMEITEM_SMTP_URL` is set but unparsable.
    /// Credentials embedded in the URL (`smtp://user:pass@host`) are applied
    /// automatically.
    pub fn from_config(config: &Config) -> AppResult<Option<Self>> {
        let Some(url) = config.smtp_url.as_deref() else {
            return Ok(None);
        };
        let builder = AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
            .map_err(|e| AppError::Internal(format!("smtp url: {e}")))?;
        // credentials embedded in the URL are applied by from_url
        Ok(Some(Self {
            transport: builder.build(),
            from: config.mail_from.clone(),
        }))
    }

    /// Send one plain-text mail.
    ///
    /// # Errors
    /// Propagates transport failures so callers can log and skip recording
    /// the notification as sent.
    pub async fn send(&self, to: &str, subject: &str, body: &str) -> AppResult<()> {
        let from = self.from.clone();
        let message = Message::builder()
            .from(from.parse().map_err(|e| AppError::Internal(format!("mail from: {e}")))?)
            .to(to.parse().map_err(|e| AppError::Internal(format!("mail to {to}: {e}")))?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(String::from(body))
            .map_err(|e| AppError::Internal(format!("mail body: {e}")))?;
        self.transport
            .send(message)
            .await
            .map_err(|e| AppError::Internal(format!("smtp send: {e}")))?;
        Ok(())
    }
}

/// Extract `user:pass` credentials from a `smtp://user:pass@host` URL.
#[cfg(test)]
fn smtp_credentials(url: &str) -> Option<Credentials> {
    let rest = url.strip_prefix("smtp://").or_else(|| url.strip_prefix("smtps://"))?;
    let auth = rest.split('/').next()?.split('?').next()?;
    let auth_part = auth.rsplit_once('@')?.0;
    let (user, pass) = auth_part.split_once(':')?;
    if user.is_empty() {
        return None;
    }
    Some(Credentials::new(user.to_string(), pass.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn disabled_without_url() {
        let config = Config::from_env_for_test();
        assert!(Mailer::from_config(&config).unwrap().is_none());
    }

    #[test]
    fn extracts_credentials() {
        let c = smtp_credentials("smtp://alice:s3cret@mail.example.com:587");
        assert!(c.is_some(), "url with credentials must yield creds");
        let none = smtp_credentials("smtp://mail.example.com:25");
        assert!(none.is_none());
    }
}
