//! `nextplane`: the single binary. `nextplane serve` (the default) runs the hub.

mod backup;
mod config;
mod service;
mod update;
mod web;

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use np_api::{AppState, VersionInfo};
use np_store::Store;

use crate::config::{Config, LogFormat, Overrides};

#[derive(Parser)]
#[command(name = "nextplane", version = version_string(), about = "Self-hosted project management in one binary")]
struct Cli {
    /// Data folder (default: ~/.nextplane)
    #[arg(long, global = true, env = "NEXTPLANE_HOME")]
    data: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the server (the default when no command is given)
    Serve {
        /// Address to listen on (default 127.0.0.1:12313)
        #[arg(long, env = "NEXTPLANE_LISTEN")]
        listen: Option<SocketAddr>,
        /// Log format: pretty or json
        #[arg(long, env = "NEXTPLANE_LOG_FORMAT")]
        log_format: Option<LogFormat>,
        /// The address people use to reach the server (for links in emails and invites)
        #[arg(long, env = "NEXTPLANE_PUBLIC_URL")]
        public_url: Option<String>,
    },
    /// Print a new one-time setup link (valid 30 minutes) while setup is not finished
    SetupLink {
        /// The address the server listens on, if not the one in config.toml
        #[arg(long, env = "NEXTPLANE_LISTEN")]
        listen: Option<SocketAddr>,
    },
    /// Install as a user service (systemd on Linux, LaunchAgent on macOS)
    Install {
        /// Print the steps without doing them
        #[arg(long)]
        dry_run: bool,
        /// Install without starting
        #[arg(long)]
        no_start: bool,
    },
    /// Stop and remove the user service (the data folder stays)
    Uninstall {
        #[arg(long)]
        dry_run: bool,
    },
    /// Is the server running?
    Status {
        #[arg(long, env = "NEXTPLANE_LISTEN")]
        listen: Option<SocketAddr>,
    },
    /// Show the service log
    Logs {
        #[arg(short, long)]
        follow: bool,
    },
    /// Write a backup archive (database, uploads, config) — safe while the server runs
    Backup {
        /// The .tar.gz file to create
        file: PathBuf,
    },
    /// Restore a backup archive (the server must be stopped)
    Restore {
        file: PathBuf,
        /// Do not ask for confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Download and install a newer release
    Update {
        /// A specific release tag, e.g. v0.2.0
        #[arg(long)]
        version: Option<String>,
        /// Only show what is installed and what is available
        #[arg(long)]
        check: bool,
    },
    /// Instance administration
    #[command(subcommand)]
    Admin(AdminCmd),
}

#[derive(Subcommand)]
enum AdminCmd {
    /// Print a password-reset link for an account (valid 24 hours)
    ResetPassword { email: String },
}

fn version_info() -> VersionInfo {
    VersionInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        commit: option_env!("NEXTPLANE_COMMIT")
            .unwrap_or("unknown")
            .to_string(),
        build_date: option_env!("NEXTPLANE_BUILD_DATE")
            .unwrap_or("unknown")
            .to_string(),
    }
}

fn version_string() -> &'static str {
    concat!(env!("CARGO_PKG_VERSION"))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve {
        listen: None,
        log_format: None,
        public_url: None,
    }) {
        Command::Serve {
            listen,
            log_format,
            public_url,
        } => {
            let cfg = config::load(&Overrides {
                data_dir: cli.data,
                listen,
                log_format,
                public_url,
            })?;
            init_tracing(cfg.log_format);
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(serve(cfg))
        }
        Command::Install { dry_run, no_start } => {
            let cfg = config::load(&Overrides {
                data_dir: cli.data,
                ..Default::default()
            })?;
            service::install(&cfg.data_dir, dry_run, no_start)
        }
        Command::Uninstall { dry_run } => {
            let cfg = config::load(&Overrides {
                data_dir: cli.data,
                ..Default::default()
            })?;
            service::uninstall(&cfg.data_dir, dry_run)
        }
        Command::Status { listen } => {
            let cfg = config::load(&Overrides {
                data_dir: cli.data,
                listen,
                ..Default::default()
            })?;
            rt()?.block_on(service::status(cfg.listen))
        }
        Command::Logs { follow } => {
            let cfg = config::load(&Overrides {
                data_dir: cli.data,
                ..Default::default()
            })?;
            service::logs(follow, &cfg.data_dir)
        }
        Command::Backup { file } => {
            let cfg = config::load(&Overrides {
                data_dir: cli.data,
                ..Default::default()
            })?;
            rt()?.block_on(async {
                let (store, _) = Store::open(&cfg.data_dir, &cfg.backups_dir()).await?;
                backup::create(&cfg, &store, &file).await?;
                store.close().await;
                println!(
                    "backup written to {} (it contains config.toml; keep it private)",
                    file.display()
                );
                Ok(())
            })
        }
        Command::Restore { file, yes } => {
            let cfg = config::load(&Overrides {
                data_dir: cli.data,
                ..Default::default()
            })?;
            rt()?.block_on(backup::restore(&cfg, &file, yes))
        }
        Command::Update { version, check } => rt()?.block_on(update::run(version, check)),
        Command::Admin(AdminCmd::ResetPassword { email }) => {
            let cfg = config::load(&Overrides {
                data_dir: cli.data,
                ..Default::default()
            })?;
            rt()?.block_on(reset_password(cfg, email))
        }
        Command::SetupLink { listen } => {
            let cfg = config::load(&Overrides {
                data_dir: cli.data,
                listen,
                ..Default::default()
            })?;
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(setup_link(cfg))
        }
    }
}

fn rt() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

async fn reset_password(cfg: Config, email: String) -> Result<()> {
    let (store, _) = Store::open(&cfg.data_dir, &cfg.backups_dir()).await?;
    let email = np_core::user::normalize_email(&email).map_err(|e| anyhow::anyhow!(e.message))?;
    let user = store
        .user_by_email(&email)
        .await
        .map_err(|e| anyhow::anyhow!(e))?
        .with_context(|| format!("no account with {email}"))?;
    let token = np_api::auth::random_token();
    store
        .create_password_reset(
            user.id,
            &np_api::auth::token_hash(&token),
            &np_api::auth::expires_in(time::Duration::hours(24)),
            None,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let base = cfg
        .public_url
        .clone()
        .unwrap_or_else(|| local_url(cfg.listen));
    println!("Reset link for {email} (valid 24 hours, single use):\n\n  {base}/reset/{token}\n");
    store.close().await;
    Ok(())
}

/// The address a person on this machine types to reach the server.
pub fn local_url(listen: SocketAddr) -> String {
    let host = if listen.ip().is_unspecified() {
        "127.0.0.1".to_string()
    } else {
        listen.ip().to_string()
    };
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host
    };
    format!("http://{host}:{}", listen.port())
}

async fn setup_link(cfg: Config) -> Result<()> {
    let (store, _) = Store::open(&cfg.data_dir, &cfg.backups_dir()).await?;
    if store
        .instance()
        .await
        .map_err(|e| anyhow::anyhow!(e))?
        .setup_completed
    {
        println!(
            "Setup is already complete. Log in at {}/login",
            local_url(cfg.listen)
        );
        return Ok(());
    }
    let token = np_api::issue_setup_token(&store)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    println!(
        "Open this link within 30 minutes to finish setup:\n\n  {}/setup?token={token}\n",
        local_url(cfg.listen)
    );
    store.close().await;
    Ok(())
}

fn init_tracing(format: LogFormat) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("NEXTPLANE_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match format {
        LogFormat::Pretty => builder.init(),
        LogFormat::Json => builder.json().init(),
    }
}

async fn serve(cfg: Config) -> Result<()> {
    let version = version_info();
    tracing::info!(version = %version.version, commit = %version.commit, data = %cfg.data_dir.display(), "starting nextplane");

    let _lock = backup::lock_data_dir(&cfg.data_dir)?;
    let (store, report) = Store::open(&cfg.data_dir, &cfg.backups_dir()).await?;
    if let Some(b) = &report.backup {
        tracing::info!(path = %b.display(), "database backed up before migrating");
    }
    if !report.applied.is_empty() {
        tracing::info!(versions = ?report.applied, created = report.created, "migrations applied");
    }

    tokio::spawn(maintenance_hourly(store.clone(), cfg.backups_dir()));
    let mut state = AppState::new(store.clone(), version);
    state.public_url = cfg.public_url.clone();
    state.uploads_dir = cfg.data_dir.join("uploads");
    state.oidc = cfg.oidc.clone();
    if let Some(o) = &cfg.oidc {
        tracing::info!(issuer = %o.issuer, "single sign-on enabled");
    }
    if let Some(smtp) = &cfg.smtp {
        state.mailer =
            Some(np_api::mail::Mailer::new(smtp).context("smtp settings in config.toml")?);
        tracing::info!(host = %smtp.host, "outgoing email enabled");
    }
    if let Some(mailer) = state.mailer.clone() {
        let base = cfg
            .public_url
            .clone()
            .unwrap_or_else(|| local_url(cfg.listen));
        tokio::spawn(digest_loop(store.clone(), mailer, base));
    }
    tokio::spawn(automation_loop(store.clone(), state.events.clone()));
    np_api::webhooks::spawn(
        store.clone(),
        state.events.clone(),
        state.http.clone(),
        Some(
            cfg.public_url
                .clone()
                .unwrap_or_else(|| local_url(cfg.listen)),
        ),
        state.version.version.clone(),
    );
    let app = np_api::router(state, web::router());
    let listener = tokio::net::TcpListener::bind(cfg.listen)
        .await
        .with_context(|| format!("listen on {}", cfg.listen))?;
    tracing::info!("listening on http://{}", listener.local_addr()?);
    let instance = store.instance().await.map_err(|e| anyhow::anyhow!(e))?;
    if !instance.setup_completed {
        let token = np_api::issue_setup_token(&store)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        let link = format!("{}/setup?token={token}", local_url(listener.local_addr()?));
        tracing::info!("setup required; the one-time setup link is printed on stderr");
        eprintln!("\n  Finish setup (link valid 30 minutes, single use):\n\n    {link}\n");
    }

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    store.close().await;
    tracing::info!("stopped");
    Ok(())
}

/// Days a workspace stays in the trash before it is removed for good (SPEC §6.2).
const TRASH_DAYS: i64 = 30;

async fn maintenance_hourly(store: Store, backups: PathBuf) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
    loop {
        tick.tick().await;
        match backup::daily(&store, &backups).await {
            Ok(Some(p)) => tracing::info!(path = %p.display(), "daily backup written"),
            Ok(None) => {}
            Err(e) => tracing::error!(error = %format!("{e:#}"), "daily backup failed"),
        }
        match store.purge_expired_sessions().await {
            Ok(0) => {}
            Ok(n) => tracing::info!(removed = n, "expired sessions removed"),
            Err(e) => tracing::warn!(error = %e, "could not remove expired sessions"),
        }
        let cutoff =
            np_store::fmt_ts(time::OffsetDateTime::now_utc() - time::Duration::days(TRASH_DAYS));
        match store.purge_trashed_workspaces(&cutoff).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(
                removed = n,
                days = TRASH_DAYS,
                "workspaces removed from trash"
            ),
            Err(e) => tracing::warn!(error = %e, "could not empty the workspace trash"),
        }
        // Webhook delivery logs are kept for 30 days.
        if let Err(e) = store.prune_deliveries(&cutoff).await {
            tracing::warn!(error = %e, "could not prune webhook deliveries");
        }
    }
}

/// Project automations, once a day from 01:00 UTC (as in Plane). A restart the same day runs
/// them again, which changes nothing new.
async fn automation_loop(store: Store, events: np_api::events::Bus) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(600));
    let mut last: Option<time::Date> = None;
    loop {
        tick.tick().await;
        let now = time::OffsetDateTime::now_utc();
        if now.hour() < 1 || last == Some(now.date()) {
            continue;
        }
        last = Some(now.date());
        match np_api::automations::run(&store, &events, now).await {
            Ok(r) if r.archived + r.closed == 0 => {}
            Ok(r) => tracing::info!(archived = r.archived, closed = r.closed, "project automations ran"),
            Err(e) => tracing::warn!(error = %format!("{e:#}"), "project automations failed"),
        }
    }
}

/// Sends notification digests every ten minutes.
async fn digest_loop(store: Store, mailer: np_api::mail::Mailer, base: String) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(600));
    loop {
        tick.tick().await;
        match np_api::send_digests(&store, &mailer, &base).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(emails = n, "notification digests sent"),
            Err(e) => tracing::warn!(error = %format!("{e:#}"), "notification digests failed"),
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
    tracing::info!("shutdown signal received, finishing open requests");
}
