//! Transactional email delivery for account verification and reset flows.

use anyhow::Context;
use lettre::{
    message::{Mailbox, Message},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
};

use crate::{config::SmtpConfig, Config};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailDelivery {
    Sent,
    NotConfigured,
}

pub async fn send_email_verification(
    config: &Config,
    to: &str,
    verify_url: &str,
) -> anyhow::Result<MailDelivery> {
    send_transactional(
        config,
        to,
        "Verify your Bluey email",
        &format!(
            "Welcome to Bluey.\n\nVerify this email address by opening this link:\n\n{verify_url}\n\nIf you did not request this, you can ignore this email."
        ),
    )
    .await
}

pub async fn send_password_reset(
    config: &Config,
    to: &str,
    reset_url: &str,
) -> anyhow::Result<MailDelivery> {
    send_transactional(
        config,
        to,
        "Reset your Bluey password",
        &format!(
            "Reset your Bluey password by opening this link:\n\n{reset_url}\n\nIf you did not request this, you can ignore this email."
        ),
    )
    .await
}

async fn send_transactional(
    config: &Config,
    to: &str,
    subject: &str,
    body: &str,
) -> anyhow::Result<MailDelivery> {
    let Some(smtp) = &config.smtp else {
        return Ok(MailDelivery::NotConfigured);
    };

    let from = smtp
        .from
        .parse::<Mailbox>()
        .context("invalid BLUEY_SMTP_FROM mailbox")?;
    let to = to.parse::<Mailbox>().context("invalid recipient mailbox")?;
    let message = Message::builder()
        .from(from)
        .to(to)
        .subject(subject)
        .body(body.to_string())
        .context("build email message")?;

    transport(smtp)?.send(message).await.context("send email")?;
    Ok(MailDelivery::Sent)
}

fn transport(smtp: &SmtpConfig) -> anyhow::Result<AsyncSmtpTransport<Tokio1Executor>> {
    let mut builder = if smtp.starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)
            .context("create STARTTLS SMTP transport")?
            .port(smtp.port)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host).port(smtp.port)
    };

    if let (Some(username), Some(password)) = (&smtp.username, &smtp.password) {
        builder = builder.credentials(Credentials::new(username.clone(), password.clone()));
    }

    Ok(builder.build())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            port: 0,
            db_path: std::path::PathBuf::from(":memory:"),
            jwt_secret: "test_secret_at_least_32_chars_long_xx".to_string(),
            public_url: "http://localhost".to_string(),
            stripe_secret_key: None,
            stripe_webhook_secret: None,
            upstream: crate::config::UpstreamKeys::default(),
            smtp: None,
        }
    }

    #[tokio::test]
    async fn verification_email_is_noop_when_smtp_unconfigured() {
        let result = send_email_verification(
            &test_config(),
            "user@example.com",
            "http://localhost/verify",
        )
        .await
        .unwrap();
        assert_eq!(result, MailDelivery::NotConfigured);
    }

    #[tokio::test]
    async fn reset_email_is_noop_when_smtp_unconfigured() {
        let result =
            send_password_reset(&test_config(), "user@example.com", "http://localhost/reset")
                .await
                .unwrap();
        assert_eq!(result, MailDelivery::NotConfigured);
    }
}
