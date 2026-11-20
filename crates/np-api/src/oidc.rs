//! Sign-in with an OpenID Connect provider (Google, Microsoft, Keycloak, Authentik, …),
//! configured in `[oidc]` of config.toml. Authorization code flow with PKCE; the user is
//! matched to an account by verified email.
//!
//! The ID token comes straight from the provider's token endpoint over TLS, so (as OIDC Core
//! §3.1.3.7 allows) its signature is not checked; issuer, audience, expiry and nonce are.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidcConfig {
    /// e.g. `https://accounts.google.com`
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    /// Shown on the button: "Sign in with <name>".
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default = "default_scopes")]
    pub scopes: String,
}

fn default_name() -> String {
    "SSO".into()
}
fn default_scopes() -> String {
    "openid email profile".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: Option<String>,
}

struct Pending {
    verifier: String,
    nonce: String,
    next: String,
    at: Instant,
}

/// Login attempts in flight, by `state`. Single use, ten minutes.
#[derive(Default)]
pub struct PendingLogins(Mutex<HashMap<String, Pending>>);

const TTL: Duration = Duration::from_secs(600);

impl PendingLogins {
    pub fn put(&self, state: &str, verifier: &str, nonce: &str, next: &str) {
        let mut m = self.0.lock().unwrap_or_else(|e| e.into_inner());
        m.retain(|_, p| p.at.elapsed() < TTL);
        if m.len() > 10_000 {
            m.clear();
        }
        m.insert(
            state.into(),
            Pending {
                verifier: verifier.into(),
                nonce: nonce.into(),
                next: next.into(),
                at: Instant::now(),
            },
        );
    }

    /// (verifier, nonce, next) for a state, removing it.
    pub fn take(&self, state: &str) -> Option<(String, String, String)> {
        let p = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(state)?;
        (p.at.elapsed() < TTL).then_some((p.verifier, p.nonce, p.next))
    }
}

pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub async fn discover(client: &reqwest::Client, issuer: &str) -> anyhow::Result<Discovery> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    let d: Discovery = client
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if d.issuer.trim_end_matches('/') != issuer.trim_end_matches('/') {
        anyhow::bail!("the provider says its issuer is {}, not {issuer}", d.issuer);
    }
    Ok(d)
}

#[derive(Debug, Deserialize)]
pub struct Claims {
    pub iss: String,
    pub aud: serde_json::Value,
    pub exp: i64,
    pub nonce: Option<String>,
    pub email: Option<String>,
    pub email_verified: Option<serde_json::Value>,
    pub name: Option<String>,
}

/// Reads and checks the ID token's claims (see the module comment about the signature).
pub fn check_id_token(
    token: &str,
    cfg: &OidcConfig,
    nonce: &str,
    now: i64,
) -> anyhow::Result<Claims> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("malformed ID token"))?;
    let c: Claims =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('='))?)?;
    if c.iss.trim_end_matches('/') != cfg.issuer.trim_end_matches('/') {
        anyhow::bail!("ID token from the wrong issuer");
    }
    let aud_ok = match &c.aud {
        serde_json::Value::String(a) => a == &cfg.client_id,
        serde_json::Value::Array(a) => a.iter().any(|x| x == &cfg.client_id),
        _ => false,
    };
    if !aud_ok {
        anyhow::bail!("ID token is for another application");
    }
    if c.exp + 60 < now {
        anyhow::bail!("ID token expired");
    }
    if c.nonce.as_deref() != Some(nonce) {
        anyhow::bail!("ID token nonce does not match");
    }
    Ok(c)
}

/// `true`, `"true"` (some providers send strings).
pub fn verified(v: &Option<serde_json::Value>) -> bool {
    matches!(v, Some(serde_json::Value::Bool(true)))
        || matches!(v, Some(serde_json::Value::String(s)) if s == "true")
}

/// Only same-site relative paths are followed after sign-in.
pub fn safe_next(next: &str) -> String {
    if next.starts_with('/')
        && !next.starts_with("//")
        && !next.starts_with("/\\")
        && !next.contains("://")
    {
        next.into()
    } else {
        "/".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> OidcConfig {
        OidcConfig {
            issuer: "https://id.example.com".into(),
            client_id: "np".into(),
            client_secret: "s".into(),
            name: "x".into(),
            scopes: default_scopes(),
        }
    }

    fn token(claims: serde_json::Value) -> String {
        format!("e30.{}.sig", URL_SAFE_NO_PAD.encode(claims.to_string()))
    }

    #[test]
    fn id_token_checks() {
        let ok = serde_json::json!({ "iss": "https://id.example.com/", "aud": ["np", "other"], "exp": 2000, "nonce": "n1", "email": "a@x.io", "email_verified": "true" });
        let c = check_id_token(&token(ok.clone()), &cfg(), "n1", 1000).unwrap();
        assert!(verified(&c.email_verified));
        let mut bad = ok.clone();
        bad["aud"] = serde_json::json!("other");
        assert!(check_id_token(&token(bad), &cfg(), "n1", 1000).is_err());
        assert!(
            check_id_token(&token(ok.clone()), &cfg(), "n2", 1000).is_err(),
            "nonce"
        );
        assert!(
            check_id_token(&token(ok), &cfg(), "n1", 5000).is_err(),
            "expired"
        );
        assert_eq!(safe_next("/acme/inbox"), "/acme/inbox");
        for evil in [
            "//evil.test",
            "https://evil.test",
            "/\\evil.test",
            "javascript:x",
        ] {
            assert_eq!(safe_next(evil), "/");
        }
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn pending_logins_are_single_use() {
        let p = PendingLogins::default();
        p.put("s", "v", "n", "/x");
        assert_eq!(p.take("s").unwrap().2, "/x");
        assert!(p.take("s").is_none());
    }
}
