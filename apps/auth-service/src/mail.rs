use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::Config;

/// Sends a plain-text mail. Without `SMTP_URL` the mail is logged instead, which is how
/// verification and reset links are picked up in local development.
pub async fn send(cfg: &Config, to: &str, subject: &str, body: &str) {
    let Some(url) = &cfg.smtp_url else {
        tracing::info!("SMTP_URL unset, mail to {to} not sent: {subject}\n{body}");
        return;
    };
    let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
        let msg = Message::builder()
            .from(cfg.mail_from.parse()?)
            .to(to.parse()?)
            .subject(subject)
            .body(body.to_string())?;
        // ponytail: one SMTP connection per mail. Upgrade: keep a pooled transport in AppState if volume grows.
        AsyncSmtpTransport::<Tokio1Executor>::from_url(url)?.build().send(msg).await?;
        Ok(())
    }
    .await;
    if let Err(e) = result {
        tracing::error!("failed to send mail to {to}: {e}");
    }
}
