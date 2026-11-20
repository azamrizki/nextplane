//! Configuration: `<data>/config.toml`, then environment variables, then command-line flags.
//!
//! The data folder is `--data`, else `$NEXTPLANE_HOME`, else `~/.nextplane`. It is created with
//! mode 0700 because it holds the database and, later, secrets.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub const DEFAULT_LISTEN: &str = "127.0.0.1:12313";
pub const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Pretty,
    Json,
}

impl std::str::FromStr for LogFormat {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "pretty" => Ok(Self::Pretty),
            "json" => Ok(Self::Json),
            other => bail!("unknown log format {other:?} (use pretty or json)"),
        }
    }
}

/// What lives in `config.toml`. Every field is optional in the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FileConfig {
    pub listen: SocketAddr,
    pub log_format: LogFormat,
    /// The address people use to reach the server, e.g. `https://plane.example.com`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    /// Outgoing email; without it invites and resets are shared as links.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smtp: Option<np_api::mail::SmtpConfig>,
    /// Optional single sign-on with an OpenID Connect provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oidc: Option<np_api::oidc::OidcConfig>,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            listen: DEFAULT_LISTEN.parse().expect("valid default"),
            log_format: LogFormat::Pretty,
            public_url: None,
            smtp: None,
            oidc: None,
        }
    }
}

/// The resolved configuration the server runs with.
#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub listen: SocketAddr,
    pub log_format: LogFormat,
    pub public_url: Option<String>,
    pub smtp: Option<np_api::mail::SmtpConfig>,
    pub oidc: Option<np_api::oidc::OidcConfig>,
}

impl Config {
    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }
}

/// Overrides from the environment or flags; `None` keeps the file's value.
#[derive(Debug, Default, Clone)]
pub struct Overrides {
    pub data_dir: Option<PathBuf>,
    pub public_url: Option<String>,
    pub listen: Option<SocketAddr>,
    pub log_format: Option<LogFormat>,
}

pub fn default_data_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set; pass --data")?;
    Ok(PathBuf::from(home).join(".nextplane"))
}

/// Resolves the data folder, creates it (0700) and `config.toml` when missing, and applies
/// overrides on top of the file.
pub fn load(over: &Overrides) -> Result<Config> {
    let data_dir = match &over.data_dir {
        Some(d) => d.clone(),
        None => default_data_dir()?,
    };
    ensure_private_dir(&data_dir)?;
    let file = read_or_create(&data_dir.join(CONFIG_FILE))?;
    Ok(Config {
        data_dir,
        listen: over.listen.unwrap_or(file.listen),
        log_format: over.log_format.unwrap_or(file.log_format),
        public_url: over
            .public_url
            .clone()
            .or(file.public_url)
            .map(|u| u.trim_end_matches('/').to_string()),
        smtp: file.smtp,
        oidc: file.oidc,
    })
}

fn read_or_create(path: &Path) -> Result<FileConfig> {
    if path.exists() {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        return toml::from_str(&text).with_context(|| format!("parse {}", path.display()));
    }
    let cfg = FileConfig::default();
    let text = format!(
        "# NextPlane configuration. Environment variables and flags override these values.\n\
         # NEXTPLANE_LISTEN / --listen, NEXTPLANE_LOG_FORMAT / --log-format,\n\
         # NEXTPLANE_PUBLIC_URL / --public-url.\n\
         #\n\
         # public_url = \"https://plane.example.com\"   # the address people use; links use it\n\
         #\n\
         # [smtp]                                       # optional outgoing email\n\
         # host = \"smtp.example.com\"\n\
         # port = 587\n\
         # username = \"...\"\n\
         # password = \"...\"\n\
         # from = \"NextPlane <noreply@example.com>\"\n\
         # security = \"starttls\"                      # starttls | tls | none\n\
         #\n\
         # [oidc]                                       # optional single sign-on\n\
         # issuer = \"https://accounts.google.com\"\n\
         # client_id = \"...\"\n\
         # client_secret = \"...\"\n\
         # name = \"Google\"                            # button: Sign in with Google\n\
         # redirect URI to register: <public_url>/api/v1/auth/oidc/callback\n\n{}",
        toml::to_string(&cfg)?
    );
    write_private(path, &text)?;
    Ok(cfg)
}

fn ensure_private_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    if !dir.is_dir() {
        bail!("{} is not a folder", dir.display());
    }
    Ok(())
}

fn write_private(path: &Path, text: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("create {}", path.display()))?;
        f.write_all(text.as_bytes())?;
    }
    #[cfg(not(unix))]
    std::fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_private_data_dir_and_default_config() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("np");
        let cfg = load(&Overrides {
            data_dir: Some(data.clone()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(cfg.listen.to_string(), DEFAULT_LISTEN);
        assert_eq!(cfg.log_format, LogFormat::Pretty);
        assert!(data.join(CONFIG_FILE).exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&data).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
            let mode = std::fs::metadata(data.join(CONFIG_FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn file_values_and_overrides() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(CONFIG_FILE),
            "listen = \"0.0.0.0:9000\"\nlog_format = \"json\"\n",
        )
        .unwrap();
        let base = Overrides {
            data_dir: Some(tmp.path().into()),
            ..Default::default()
        };
        let cfg = load(&base).unwrap();
        assert_eq!(cfg.listen.to_string(), "0.0.0.0:9000");
        assert_eq!(cfg.log_format, LogFormat::Json);

        let over = Overrides {
            listen: Some("127.0.0.1:1".parse().unwrap()),
            ..base
        };
        assert_eq!(load(&over).unwrap().listen.to_string(), "127.0.0.1:1");
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(CONFIG_FILE), "lisen = \"x\"\n").unwrap();
        let err = load(&Overrides {
            data_dir: Some(tmp.path().into()),
            ..Default::default()
        });
        assert!(err.is_err());
    }
}
