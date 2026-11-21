//! Sign-in providers the instance admin turns on (S3.7, as in Plane): Google, GitHub, GitLab and
//! Gitea, next to the generic OpenID provider of config.toml. Google, GitLab and Gitea speak
//! OpenID Connect; GitHub is plain OAuth with its own user API. Settings live in the
//! `instance_config` table under `auth.`; the client secret never leaves the server.

use np_store::Store;

use crate::oidc::OidcConfig;

pub const KEYS: [&str; 4] = ["google", "github", "gitlab", "gitea"];

#[derive(Debug, Clone, Default)]
pub struct Provider {
    pub key: &'static str,
    pub enabled: bool,
    pub client_id: String,
    pub client_secret: String,
    /// GitLab and Gitea: the server (`https://gitlab.com`); GitHub: an Enterprise server, if any.
    pub host: String,
    /// GitHub: only members of this organization may sign in.
    pub org: String,
}

impl Provider {
    pub fn name(&self) -> &'static str {
        match self.key {
            "google" => "Google",
            "github" => "GitHub",
            "gitlab" => "GitLab",
            _ => "Gitea",
        }
    }

    /// Client id and secret, and a host where one is needed.
    pub fn configured(&self) -> bool {
        !self.client_id.is_empty()
            && !self.client_secret.is_empty()
            && (!matches!(self.key, "gitlab" | "gitea") || !self.host.is_empty())
    }

    pub fn usable(&self) -> bool {
        self.enabled && self.configured()
    }

    /// The OpenID Connect settings of Google, GitLab and Gitea (GitHub has none).
    pub fn oidc(&self) -> Option<OidcConfig> {
        let issuer = match self.key {
            "google" => "https://accounts.google.com".to_string(),
            "gitlab" | "gitea" => self.host.trim_end_matches('/').to_string(),
            _ => return None,
        };
        Some(OidcConfig {
            issuer,
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            name: self.name().into(),
            scopes: "openid email profile".into(),
        })
    }

    /// GitHub's web and API bases: github.com, or an Enterprise server's `/api/v3`.
    pub fn github_bases(&self) -> (String, String) {
        let host = self.host.trim_end_matches('/');
        if host.is_empty() || host == "https://github.com" {
            ("https://github.com".into(), "https://api.github.com".into())
        } else {
            (host.into(), format!("{host}/api/v3"))
        }
    }
}

/// Every provider with its saved settings.
pub async fn load(store: &Store) -> np_core::Result<Vec<Provider>> {
    let c = store.config("auth.").await?;
    let get = |p: &str, k: &str| c.get(&format!("auth.{p}.{k}")).cloned().unwrap_or_default();
    Ok(KEYS
        .iter()
        .map(|&key| Provider {
            key,
            enabled: get(key, "enabled") == "1",
            client_id: get(key, "client_id"),
            client_secret: get(key, "client_secret"),
            host: get(key, "host"),
            org: get(key, "org"),
        })
        .collect())
}

pub async fn find(store: &Store, key: &str) -> np_core::Result<Option<Provider>> {
    Ok(load(store).await?.into_iter().find(|p| p.key == key))
}

/// Email and password sign-in (on unless the admin turned it off).
pub async fn password_enabled(store: &Store) -> bool {
    store
        .config("auth.password_enabled")
        .await
        .map(|c| c.get("auth.password_enabled").map(String::as_str) != Some("0"))
        .unwrap_or(true)
}
