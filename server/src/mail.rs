//! Transactional email delivery for account verification and reset flows.

use anyhow::Context;
use lettre::{
    message::{header::ContentType, Mailbox, Message, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
};
use serde_json::{json, Value};
use std::time::Duration;

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
        None,
    )
    .await
}

pub async fn send_signup_otp(
    config: &Config,
    to: &str,
    code: &str,
    expires_in_minutes: i64,
) -> anyhow::Result<MailDelivery> {
    let text_body = format!(
        "Welcome to Bluey.\n\nYour verification code is:\n\n{code}\n\nThis code expires in {expires_in_minutes} minutes. If you did not request this, you can ignore this email."
    );
    let html_body = signup_otp_email_html(code, expires_in_minutes);
    send_transactional(
        config,
        to,
        "Your Bluey verification code",
        &text_body,
        Some(&html_body),
    )
    .await
}

pub async fn send_jobs_identity_otp(
    config: &Config,
    to: &str,
    code: &str,
    expires_in_minutes: i64,
) -> anyhow::Result<MailDelivery> {
    let text_body = format!(
        "Verify this application email for Bluey Jobs.\n\nYour verification code is:\n\n{code}\n\nThis code expires in {expires_in_minutes} minutes. If you did not add this address to Bluey Jobs, you can ignore this email."
    );
    let html_body = otp_email_html(
        code,
        expires_in_minutes,
        "application email",
        "Verify your application email.",
        "Use this code to confirm Bluey can use this address on resumes and job applications.",
    );
    send_transactional(
        config,
        to,
        "Verify your Bluey Jobs application email",
        &text_body,
        Some(&html_body),
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
        None,
    )
    .await
}

async fn send_transactional(
    config: &Config,
    to: &str,
    subject: &str,
    body: &str,
    html_body: Option<&str>,
) -> anyhow::Result<MailDelivery> {
    let Some(smtp) = &config.smtp else {
        return Ok(MailDelivery::NotConfigured);
    };

    let from = smtp
        .from
        .parse::<Mailbox>()
        .context("invalid BLUEY_SMTP_FROM mailbox")?;
    let to_mailbox = to.parse::<Mailbox>().context("invalid recipient mailbox")?;
    if uses_resend_api(smtp) {
        return send_resend_api(smtp, to, subject, body, html_body).await;
    }

    let builder = Message::builder()
        .from(from)
        .to(to_mailbox)
        .subject(subject);
    let message = if let Some(html_body) = html_body {
        builder
            .multipart(
                MultiPart::alternative()
                    .singlepart(SinglePart::plain(body.to_string()))
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html_body.to_string()),
                    ),
            )
            .context("build html email message")?
    } else {
        builder
            .body(body.to_string())
            .context("build email message")?
    };

    transport(smtp)?.send(message).await.context("send email")?;
    Ok(MailDelivery::Sent)
}

fn uses_resend_api(smtp: &SmtpConfig) -> bool {
    std::env::var("BLUEY_MAIL_TRANSPORT")
        .map(|value| value.eq_ignore_ascii_case("resend"))
        .unwrap_or_else(|_| smtp.host.eq_ignore_ascii_case("smtp.resend.com"))
}

async fn send_resend_api(
    smtp: &SmtpConfig,
    to: &str,
    subject: &str,
    body: &str,
    html_body: Option<&str>,
) -> anyhow::Result<MailDelivery> {
    let api_key = smtp
        .password
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .context("Resend API key missing from BLUEY_SMTP_PASSWORD")?;
    let base_url = std::env::var("BLUEY_RESEND_API_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "https://api.resend.com".to_string());
    let url = format!("{}/emails", base_url.trim_end_matches('/'));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .context("build Resend API client")?;
    let mut payload = json!({
        "from": smtp.from,
        "to": [to],
        "subject": subject,
        "text": body,
    });
    if let (Some(html_body), Some(object)) = (html_body, payload.as_object_mut()) {
        object.insert("html".to_string(), Value::String(html_body.to_string()));
    }

    let response = client
        .post(url)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .context("send Resend API email")?;

    if !response.status().is_success() {
        anyhow::bail!("Resend API email failed with status {}", response.status());
    }

    Ok(MailDelivery::Sent)
}

fn signup_otp_email_html(code: &str, expires_in_minutes: i64) -> String {
    otp_email_html(
        code,
        expires_in_minutes,
        "secure sign in",
        "Welcome to Bluey.",
        "Use this code to finish signing in. Keep it private; Bluey will never ask you to share it with anyone.",
    )
}

fn otp_email_html(
    code: &str,
    expires_in_minutes: i64,
    eyebrow: &str,
    title: &str,
    copy: &str,
) -> String {
    let code = escape_html(&spaced_verification_code(code));
    let eyebrow = escape_html(eyebrow);
    let title = escape_html(title);
    let copy = escape_html(copy);
    format!(
        r#"<!doctype html>
<html>
  <body style="margin:0;padding:0;background:#05080d;color:#f4fbff;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif">
    <div style="display:none;max-height:0;overflow:hidden;color:transparent;opacity:0">Your Bluey verification code expires in {expires_in_minutes} minutes.</div>
    <table role="presentation" width="100%" cellspacing="0" cellpadding="0" style="margin:0;padding:0;background:#05080d">
      <tr>
        <td align="center" style="padding:28px 14px">
          <table role="presentation" width="100%" cellspacing="0" cellpadding="0" style="width:100%;max-width:560px;background:#08111c;border:1px solid #124d6b;border-radius:26px;box-shadow:0 18px 48px rgba(0,0,0,.42),0 0 34px rgba(80,202,255,.16);overflow:hidden">
            <tr>
              <td style="padding:28px 28px 18px">
                <table role="presentation" cellspacing="0" cellpadding="0">
                  <tr>
                    <td width="52" height="52" align="center" valign="middle" style="width:52px;height:52px;border-radius:16px;background:#06111f;border:1px solid #1b6d8a;box-shadow:inset 0 0 0 1px rgba(103,223,255,.14),0 0 22px rgba(103,223,255,.24)">
                      <span style="display:inline-block;color:#67dfff;font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:22px;font-weight:800;line-height:52px">&gt;_</span>
                    </td>
                    <td style="padding-left:14px">
                      <div style="font-size:42px;line-height:1;font-weight:850;letter-spacing:-.01em">
                        <span style="color:#e9f7ff">blu</span><span style="color:#39c7ff">ey</span>
                      </div>
                      <div style="padding-top:6px;color:#8fb3c7;font-size:13px;line-height:1.35;font-weight:700;letter-spacing:.12em;text-transform:uppercase">{eyebrow}</div>
                    </td>
                  </tr>
                </table>
              </td>
            </tr>
            <tr>
              <td style="padding:6px 28px 0">
                <div style="height:1px;background:#123047"></div>
              </td>
            </tr>
            <tr>
              <td style="padding:26px 28px 10px">
                <h1 style="margin:0 0 10px;color:#f4fbff;font-size:28px;line-height:1.18;font-weight:850">{title}</h1>
                <p style="margin:0;color:#aec1ce;font-size:16px;line-height:1.55">{copy}</p>
              </td>
            </tr>
            <tr>
              <td style="padding:18px 28px 18px">
                <table role="presentation" width="100%" cellspacing="0" cellpadding="0" style="background:#0b1826;border:1px solid #1f83a6;border-radius:20px;box-shadow:inset 0 0 0 1px rgba(103,223,255,.10),0 0 26px rgba(103,223,255,.14)">
                  <tr>
                    <td align="center" style="padding:14px 18px 0;color:#67dfff;font-size:12px;line-height:1.4;font-weight:850;letter-spacing:.16em;text-transform:uppercase">verification code</td>
                  </tr>
                  <tr>
                    <td align="center" style="padding:10px 18px 22px;color:#ffffff;font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:46px;line-height:1.1;font-weight:850;letter-spacing:.18em">{code}</td>
                  </tr>
                </table>
              </td>
            </tr>
            <tr>
              <td style="padding:0 28px 26px">
                <table role="presentation" cellspacing="0" cellpadding="0" style="background:#07131f;border:1px solid #14344a;border-radius:999px">
                  <tr>
                    <td style="padding:9px 13px;color:#b9cad5;font-size:14px;line-height:1.4;font-weight:700">This code expires in {expires_in_minutes} minutes.</td>
                  </tr>
                </table>
                <p style="margin:18px 0 0;color:#718898;font-size:13px;line-height:1.5">If you did not request this code, you can safely ignore this email.</p>
              </td>
            </tr>
          </table>
        </td>
      </tr>
    </table>
  </body>
</html>"#
    )
}

fn spaced_verification_code(code: &str) -> String {
    code.chars()
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
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
    use serial_test::serial;
    use wiremock::{
        matchers::{body_json, body_partial_json, body_string_contains, header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn test_config() -> Config {
        Config {
            port: 0,
            db_path: std::path::PathBuf::from(":memory:"),
            db_backend: crate::config::ServerDbBackend::Sqlite,
            database_url: None,
            jwt_secret: "test_secret_at_least_32_chars_long_xx".to_string(),
            public_url: "http://localhost".to_string(),
            stripe_secret_key: None,
            stripe_webhook_secret: None,
            upstream: crate::config::UpstreamKeys::default(),
            upstream_spend_guard: None,
            smtp: None,
            admin_emails: vec![],
            trial_abuse: crate::config::TrialAbuseConfig::default(),
            turnstile_site_key: None,
            turnstile_secret_key: None,
            require_turnstile: false,
            object_storage: None,
            log_storage: None,
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

    #[tokio::test]
    async fn signup_otp_email_is_noop_when_smtp_unconfigured() {
        let result = send_signup_otp(&test_config(), "user@example.com", "123456", 10)
            .await
            .unwrap();
        assert_eq!(result, MailDelivery::NotConfigured);
    }

    #[test]
    fn signup_otp_email_html_uses_bluey_brand_and_spaced_code() {
        let html = signup_otp_email_html("123456", 10);

        assert!(html.contains("Bluey"));
        assert!(html.contains("Welcome to Bluey."));
        assert!(html.contains("#67dfff"));
        assert!(html.contains("1 2 3 4 5 6"));
        assert!(html.contains("This code expires in 10 minutes."));
    }

    #[test]
    fn signup_otp_email_html_escapes_code() {
        let html = signup_otp_email_html("<12&34>", 10);

        assert!(html.contains("&lt; 1 2 &amp; 3 4 &gt;"));
        assert!(!html.contains("<12&34>"));
    }

    #[tokio::test]
    #[serial]
    async fn resend_api_transport_sends_email() {
        let server = MockServer::start().await;
        std::env::set_var("BLUEY_RESEND_API_BASE_URL", server.uri());

        Mock::given(method("POST"))
            .and(path("/emails"))
            .and(header("Authorization", "Bearer test-resend-key"))
            .and(body_json(json!({
                "from": "Bluey <noreply@bluey.sh>",
                "to": ["user@example.com"],
                "subject": "Verify your Bluey email",
                "text": "Welcome to Bluey.\n\nVerify this email address by opening this link:\n\nhttps://bluey.sh/verify\n\nIf you did not request this, you can ignore this email."
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"email-test"})))
            .mount(&server)
            .await;

        let mut config = test_config();
        config.smtp = Some(SmtpConfig {
            host: "smtp.resend.com".to_string(),
            port: 587,
            username: Some("resend".to_string()),
            password: Some("test-resend-key".to_string()),
            from: "Bluey <noreply@bluey.sh>".to_string(),
            starttls: true,
        });

        let result =
            send_email_verification(&config, "user@example.com", "https://bluey.sh/verify")
                .await
                .unwrap();
        assert_eq!(result, MailDelivery::Sent);

        std::env::remove_var("BLUEY_RESEND_API_BASE_URL");
    }

    #[tokio::test]
    #[serial]
    async fn resend_api_transport_sends_signup_otp_html() {
        let server = MockServer::start().await;
        std::env::set_var("BLUEY_RESEND_API_BASE_URL", server.uri());

        Mock::given(method("POST"))
            .and(path("/emails"))
            .and(header("Authorization", "Bearer test-resend-key"))
            .and(body_partial_json(json!({
                "from": "Bluey <noreply@bluey.sh>",
                "to": ["user@example.com"],
                "subject": "Your Bluey verification code",
                "text": "Welcome to Bluey.\n\nYour verification code is:\n\n123456\n\nThis code expires in 10 minutes. If you did not request this, you can ignore this email."
            })))
            .and(body_string_contains("\"html\""))
            .and(body_string_contains("Welcome to Bluey."))
            .and(body_string_contains("1 2 3 4 5 6"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"email-test"})))
            .mount(&server)
            .await;

        let mut config = test_config();
        config.smtp = Some(SmtpConfig {
            host: "smtp.resend.com".to_string(),
            port: 587,
            username: Some("resend".to_string()),
            password: Some("test-resend-key".to_string()),
            from: "Bluey <noreply@bluey.sh>".to_string(),
            starttls: true,
        });

        let result = send_signup_otp(&config, "user@example.com", "123456", 10)
            .await
            .unwrap();
        assert_eq!(result, MailDelivery::Sent);

        std::env::remove_var("BLUEY_RESEND_API_BASE_URL");
    }
}
