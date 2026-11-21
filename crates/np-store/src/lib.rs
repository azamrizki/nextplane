//! SQLite storage for NextPlane: one database file in WAL mode, migrations embedded in the
//! binary, and a backup of the database taken before any migration touches existing data.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use time::OffsetDateTime;

pub mod analytics;
pub mod automations;
pub mod board;
pub mod collab;
pub mod home;
pub mod cycles;
pub mod instance;
pub mod intake;
pub mod integrations;
pub mod invites;
pub mod modules;
pub mod notifications;
pub mod pages;
pub mod profile;
pub mod projects;
pub mod search;
pub mod sessions;
pub mod time_fmt;
pub mod transfer;
pub mod users;
pub mod views;
pub mod work_items;
pub mod workspaces;

pub use time_fmt::{fmt_ts, now_ts, parse_ts};

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// File name of the database inside the data folder.
pub const DB_FILE: &str = "nextplane.db";

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

/// What [`Store::open`] did, for logging.
#[derive(Debug, Default)]
pub struct OpenReport {
    pub created: bool,
    pub applied: Vec<i64>,
    pub backup: Option<PathBuf>,
}

impl Store {
    /// Opens (or creates) `<data_dir>/nextplane.db` and brings it to the latest schema.
    /// When an existing database has pending migrations, a copy is written to `backup_dir`
    /// first with `VACUUM INTO`; if that copy fails, no migration runs.
    pub async fn open(data_dir: &Path, backup_dir: &Path) -> Result<(Self, OpenReport)> {
        let path = data_dir.join(DB_FILE);
        let existed = path.exists();
        let opts = SqliteConnectOptions::from_str("sqlite:")?
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await
            .with_context(|| format!("open database {}", path.display()))?;
        let store = Self { pool };

        let mut report = OpenReport {
            created: !existed,
            ..Default::default()
        };
        let pending = store.pending_migrations().await?;
        if pending.is_empty() {
            store.ensure_instance_row().await?;
            return Ok((store, report));
        }
        if existed && store.has_tables().await? {
            report.backup = Some(store.backup_before_migrate(backup_dir, &pending).await?);
        }
        MIGRATOR.run(&store.pool).await.context("run migrations")?;
        report.applied = pending;
        store.ensure_instance_row().await?;
        Ok((store, report))
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// A cheap liveness check for `/api/v1/health`.
    pub async fn ping(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    /// Latest applied migration version, `None` on an empty database.
    pub async fn schema_version(&self) -> Result<Option<i64>> {
        if !self.migrations_table_exists().await? {
            return Ok(None);
        }
        let v: Option<i64> =
            sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(&self.pool)
                .await?;
        Ok(v)
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    async fn ensure_instance_row(&self) -> Result<()> {
        let now = now_ts();
        sqlx::query(
            "INSERT OR IGNORE INTO instance (id, name, created_at, updated_at) VALUES (1, '', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn migrations_table_exists(&self) -> Result<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(n > 0)
    }

    /// True when the file holds any table at all, so a migration could touch existing data.
    /// Deliberately broader than "has applied migrations": a database with data but a missing
    /// or damaged migration record is still backed up.
    async fn has_tables(&self) -> Result<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(n > 0)
    }

    async fn pending_migrations(&self) -> Result<Vec<i64>> {
        let applied: Vec<i64> = if self.migrations_table_exists().await? {
            sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = 1")
                .fetch_all(&self.pool)
                .await?
        } else {
            Vec::new()
        };
        Ok(MIGRATOR
            .iter()
            .filter(|m| m.migration_type.is_up_migration())
            .map(|m| m.version)
            .filter(|v| !applied.contains(v))
            .collect())
    }

    async fn backup_before_migrate(&self, backup_dir: &Path, pending: &[i64]) -> Result<PathBuf> {
        std::fs::create_dir_all(backup_dir)
            .with_context(|| format!("create backup folder {}", backup_dir.display()))?;
        let stamp = OffsetDateTime::now_utc().unix_timestamp();
        let to = pending.last().copied().unwrap_or_default();
        let target = backup_dir.join(format!("pre-migrate-{stamp}-to-{to:04}.db"));
        if target.exists() {
            anyhow::bail!("backup target {} already exists", target.display());
        }
        let target_str = target.to_str().context("backup path is not valid UTF-8")?;
        sqlx::query("VACUUM INTO ?")
            .bind(target_str)
            .execute(&self.pool)
            .await
            .with_context(|| format!("back up database to {}", target.display()))?;
        Ok(target)
    }
}

/// Maps a database error to the shared error type: unique violations become `conflict`,
/// everything else is logged and reported as `internal`.
pub(crate) fn db_err(e: sqlx::Error) -> np_core::Error {
    if let sqlx::Error::Database(d) = &e
        && d.is_unique_violation()
    {
        return np_core::Error::conflict("already exists");
    }
    tracing::error!(error = %e, "database error");
    np_core::Error::internal()
}

impl Store {
    /// A consistent copy of the live database (safe while the server runs).
    pub async fn backup_to(&self, target: &Path) -> Result<()> {
        if target.exists() {
            anyhow::bail!("{} already exists", target.display());
        }
        let t = target.to_str().context("backup path is not valid UTF-8")?;
        sqlx::query("VACUUM INTO ?")
            .bind(t)
            .execute(&self.pool)
            .await
            .with_context(|| format!("back up to {}", target.display()))?;
        Ok(())
    }
}
