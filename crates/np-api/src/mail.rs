//! Outgoing email over SMTP. Optional: without an `[smtp]` section in config.toml, invites
//! and resets work by copying links from the UI, and nothing is sent.

use lettre::message::{Mailbox, header::ContentType};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SmtpSecurity {
    /// STARTTLS on port 587 (the default).
    #[default]
    Starttls,
    /// Implicit TLS on port 465.
    Tls,
    /// No encryption: only for a relay on the same machine.
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmtpConfig {
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// `NextPlane <noreply@example.com>`
    pub from: String,
    #[serde(default)]
    pub security: SmtpSecurity,
}

#[derive(Clone)]
pub struct Mailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl Mailer {
    pub fn new(cfg: &SmtpConfig) -> anyhow::Result<Self> {
        let mut b = match cfg.security {
            SmtpSecurity::Starttls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)?
            }
            SmtpSecurity::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)?,
            SmtpSecurity::None => {
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.host)
            }
        };
        if let Some(p) = cfg.port {
            b = b.port(p);
        }
        if let (Some(u), Some(p)) = (&cfg.username, &cfg.password) {
            b = b.credentials(Credentials::new(u.clone(), p.clone()));
        }
        Ok(Self {
            transport: b.build(),
            from: cfg.from.parse()?,
        })
    }

    /// Sends in the background; failures are logged, never shown to the requester (so an
    /// email address's existence is not revealed through errors).
    pub fn send_later(&self, to: &str, subject: String, body: String) {
        let this = self.clone();
        let to = to.to_string();
        tokio::spawn(async move {
            if let Err(e) = this.send(&to, &subject, body).await {
                tracing::warn!(to = %to, error = %e, "email not sent");
            }
        });
    }

    async fn send(&self, to: &str, subject: &str, body: String) -> anyhow::Result<()> {
        let msg = Message::builder()
            .from(self.from.clone())
            .to(to.parse()?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body)?;
        self.transport.send(msg).await?;
        Ok(())
    }
}
