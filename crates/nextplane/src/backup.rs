//! Backups and restores (SPEC §10).
//!
//! * Daily: the running server copies the database to `backups/daily-YYYY-MM-DD.db` once a
//!   day and keeps the newest 14 of those files (nothing else in the folder is touched).
//! * `nextplane backup <file>`: one `.tar.gz` with the database, the uploads and
//!   `config.toml`. Safe while the server runs.
//! * `nextplane restore <file>`: only while the server is stopped, after a confirmation and
//!   a fresh backup of the current state. The replaced files are moved aside, not deleted.

use std::fs::{self, File};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use np_store::{DB_FILE, Store};

use crate::config::{CONFIG_FILE, Config};

pub const DAILY_KEEP: usize = 14;
pub const LOCK_FILE: &str = "nextplane.lock";

fn stamp() -> String {
    np_store::now_ts().replace([':', '.'], "-")
}

/// Holds an exclusive lock on `<data>/nextplane.lock` while the server runs, so a restore
/// can tell it is running. Released when dropped (or when the process ends).
pub struct RunLock(#[allow(dead_code)] File);

pub fn lock_data_dir(data: &Path) -> Result<RunLock> {
    let f = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join(LOCK_FILE))?;
    match f.try_lock() {
        Ok(()) => Ok(RunLock(f)),
        Err(_) => bail!(
            "another nextplane server is already using {}",
            data.display()
        ),
    }
}

fn server_running(data: &Path) -> bool {
    match File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join(LOCK_FILE))
    {
        Ok(f) => f.try_lock().is_err(),
        Err(_) => false,
    }
}

/// Writes today's daily copy if missing and prunes old daily copies.
pub async fn daily(store: &Store, backups: &Path) -> Result<Option<PathBuf>> {
    fs::create_dir_all(backups)?;
    let today = &np_store::now_ts()[..10];
    let target = backups.join(format!("daily-{today}.db"));
    let made = if target.exists() {
        None
    } else {
        store.backup_to(&target).await?;
        Some(target)
    };
    let mut dailies: Vec<PathBuf> = fs::read_dir(backups)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with("daily-")
                    && n.ends_with(".db")
                    && n.len() == "daily-YYYY-MM-DD.db".len()
            })
        })
        .collect();
    dailies.sort();
    while dailies.len() > DAILY_KEEP {
        let old = dailies.remove(0);
        fs::remove_file(&old)
            .with_context(|| format!("remove old daily backup {}", old.display()))?;
    }
    Ok(made)
}

/// Writes a full backup archive to `out`.
pub async fn create(cfg: &Config, store: &Store, out: &Path) -> Result<()> {
    if out.exists() {
        bail!("{} already exists; choose a new file name", out.display());
    }
    let tmp_dir = cfg.backups_dir().join(format!(".tmp-{}", stamp()));
    fs::create_dir_all(&tmp_dir)?;
    let db_copy = tmp_dir.join(DB_FILE);
    let result = async {
        store.backup_to(&db_copy).await?;
        write_archive(cfg, &db_copy, out)
    }
    .await;
    // The temporary copy is ours; remove it whatever happened.
    let _ = fs::remove_file(&db_copy);
    let _ = fs::remove_dir(&tmp_dir);
    result
}

fn write_archive(cfg: &Config, db_copy: &Path, out: &Path) -> Result<()> {
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let file = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            File::options()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(out)
        }
        #[cfg(not(unix))]
        {
            File::options().write(true).create_new(true).open(out)
        }
    }
    .with_context(|| format!("create {}", out.display()))?;
    let mut tar = tar::Builder::new(GzEncoder::new(file, Compression::default()));
    tar.append_path_with_name(db_copy, DB_FILE)?;
    let cfg_file = cfg.data_dir.join(CONFIG_FILE);
    if cfg_file.exists() {
        tar.append_path_with_name(&cfg_file, CONFIG_FILE)?;
    }
    let uploads = cfg.data_dir.join("uploads");
    if uploads.is_dir() {
        tar.append_dir_all("uploads", &uploads)?;
    }
    tar.into_inner()?.finish()?.sync_all()?;
    Ok(())
}

/// Checks an archive: it must hold a database whose schema this binary knows.
async fn verify(archive: &Path, scratch: &Path) -> Result<i64> {
    fs::create_dir_all(scratch)?;
    let mut a = tar::Archive::new(GzDecoder::new(
        File::open(archive).with_context(|| format!("open {}", archive.display()))?,
    ));
    let mut found = false;
    for entry in a.entries()? {
        let mut e = entry?;
        let path = e.path()?.into_owned();
        check_entry_path(&path)?;
        if path == Path::new(DB_FILE) {
            e.unpack(scratch.join(DB_FILE))?;
            found = true;
        }
    }
    if !found {
        bail!(
            "{} is not a NextPlane backup (no {DB_FILE} inside)",
            archive.display()
        );
    }
    let opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(scratch.join(DB_FILE))
        .read_only(true);
    let pool = sqlx::SqlitePool::connect_with(opts).await?;
    let ok: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await?;
    let version: Option<i64> =
        sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(&pool)
            .await?;
    pool.close().await;
    if ok != "ok" {
        bail!("the database in the backup is damaged: {ok}");
    }
    let latest = np_store::MIGRATOR
        .iter()
        .map(|m| m.version)
        .max()
        .unwrap_or(0);
    let v = version.unwrap_or(0);
    if v > latest {
        bail!(
            "the backup is from a newer NextPlane (schema {v}, this binary knows {latest}); update first"
        );
    }
    Ok(v)
}

/// Archive members must stay inside the data folder.
fn check_entry_path(p: &Path) -> Result<()> {
    use std::path::Component;
    if p.components().any(|c| !matches!(c, Component::Normal(_))) {
        bail!("refusing an unsafe path in the archive: {}", p.display());
    }
    Ok(())
}

/// Restores `archive` into the data folder. See the module comment for the safety steps.
pub async fn restore(cfg: &Config, archive: &Path, yes: bool) -> Result<()> {
    let data = &cfg.data_dir;
    if server_running(data) {
        bail!(
            "the server is running on {}; stop it first (systemctl --user stop nextplane)",
            data.display()
        );
    }
    let _lock = lock_data_dir(data)?;
    let ts = stamp();
    let scratch = cfg.backups_dir().join(format!(".restore-check-{ts}"));
    let version = verify(archive, &scratch).await;
    let _ = fs::remove_file(scratch.join(DB_FILE));
    let _ = fs::remove_dir(&scratch);
    let version = version?;

    println!(
        "Restore {} (schema {version}) into {}.",
        archive.display(),
        data.display()
    );
    println!(
        "The current database and uploads are backed up first and then moved to backups/replaced-{ts}/."
    );
    if !yes {
        print!("Type RESTORE to continue: ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        if line.trim() != "RESTORE" {
            bail!("restore cancelled");
        }
    }

    let had_db = data.join(DB_FILE).exists();
    if had_db {
        let (store, _) = Store::open(data, &cfg.backups_dir()).await?;
        let safety = cfg.backups_dir().join(format!("pre-restore-{ts}.tar.gz"));
        create(cfg, &store, &safety).await?;
        store.close().await;
        println!("Current state saved to {}", safety.display());
    }

    let aside = cfg.backups_dir().join(format!("replaced-{ts}"));
    fs::create_dir_all(&aside)?;
    for name in [
        DB_FILE.to_string(),
        format!("{DB_FILE}-wal"),
        format!("{DB_FILE}-shm"),
        "uploads".into(),
    ] {
        let from = data.join(&name);
        if from.exists() {
            fs::rename(&from, aside.join(&name))
                .with_context(|| format!("move {} aside", from.display()))?;
        }
    }

    let mut a = tar::Archive::new(GzDecoder::new(File::open(archive)?));
    for entry in a.entries()? {
        let mut e = entry?;
        let path = e.path()?.into_owned();
        check_entry_path(&path)?;
        // config.toml stays as it is on this machine (addresses and secrets differ per host).
        if path == Path::new(CONFIG_FILE) {
            continue;
        }
        e.unpack_in(data)?;
    }
    println!("Restored. Start the server again (systemctl --user start nextplane).");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(dir: &Path) -> Config {
        crate::config::load(&crate::config::Overrides {
            data_dir: Some(dir.to_path_buf()),
            ..Default::default()
        })
        .unwrap()
    }

    #[tokio::test]
    async fn backup_then_restore_round_trip_keeps_old_files() {
        let tmp = tempfile::tempdir().unwrap();
        let c = cfg(&tmp.path().join("data"));
        let (store, _) = Store::open(&c.data_dir, &c.backups_dir()).await.unwrap();
        store.update_instance(Some("Before"), None).await.unwrap();
        fs::create_dir_all(c.data_dir.join("uploads/2026-11")).unwrap();
        fs::write(c.data_dir.join("uploads/2026-11/a"), b"file").unwrap();
        let archive = tmp.path().join("b.tar.gz");
        create(&c, &store, &archive).await.unwrap();
        assert!(
            create(&c, &store, &archive).await.is_err(),
            "never overwrites"
        );

        // Change things after the backup.
        store.update_instance(Some("After"), None).await.unwrap();
        fs::write(c.data_dir.join("uploads/2026-11/b"), b"new").unwrap();
        store.close().await;

        restore(&c, &archive, true).await.unwrap();
        let (store, _) = Store::open(&c.data_dir, &c.backups_dir()).await.unwrap();
        assert_eq!(store.instance().await.unwrap().name, "Before");
        assert!(c.data_dir.join("uploads/2026-11/a").exists());
        assert!(!c.data_dir.join("uploads/2026-11/b").exists());
        // The replaced state still exists, twice: as an archive and as moved files.
        let names: Vec<String> = fs::read_dir(c.backups_dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(
            names.iter().any(|n| n.starts_with("pre-restore-")),
            "{names:?}"
        );
        let replaced = names.iter().find(|n| n.starts_with("replaced-")).unwrap();
        assert!(
            c.backups_dir()
                .join(replaced)
                .join("uploads/2026-11/b")
                .exists()
        );
    }

    #[tokio::test]
    async fn restore_refuses_while_running_and_bad_archives() {
        let tmp = tempfile::tempdir().unwrap();
        let c = cfg(&tmp.path().join("data"));
        let (store, _) = Store::open(&c.data_dir, &c.backups_dir()).await.unwrap();
        let archive = tmp.path().join("b.tar.gz");
        create(&c, &store, &archive).await.unwrap();
        {
            let _running = lock_data_dir(&c.data_dir).unwrap();
            let err = restore(&c, &archive, true).await.unwrap_err();
            assert!(err.to_string().contains("running"), "{err}");
        }
        fs::write(tmp.path().join("junk.tar.gz"), b"not an archive").unwrap();
        assert!(
            restore(&c, &tmp.path().join("junk.tar.gz"), true)
                .await
                .is_err()
        );
        assert!(check_entry_path(Path::new("../etc/passwd")).is_err());
        assert!(check_entry_path(Path::new("/etc/passwd")).is_err());
        assert!(check_entry_path(Path::new("uploads/x")).is_ok());
    }

    #[tokio::test]
    async fn daily_keeps_fourteen_and_touches_nothing_else() {
        let tmp = tempfile::tempdir().unwrap();
        let c = cfg(&tmp.path().join("data"));
        let (store, _) = Store::open(&c.data_dir, &c.backups_dir()).await.unwrap();
        let b = c.backups_dir();
        fs::create_dir_all(&b).unwrap();
        for d in 1..=20 {
            fs::write(b.join(format!("daily-2020-01-{d:02}.db")), b"x").unwrap();
        }
        fs::write(b.join("pre-migrate-1-to-0002.db"), b"keep").unwrap();
        fs::write(b.join("daily-notes.txt"), b"keep").unwrap();
        assert!(daily(&store, &b).await.unwrap().is_some());
        assert!(daily(&store, &b).await.unwrap().is_none(), "once a day");
        let dailies = fs::read_dir(&b)
            .unwrap()
            .filter(|e| {
                let n = e.as_ref().unwrap().file_name().into_string().unwrap();
                n.starts_with("daily-") && n.ends_with(".db")
            })
            .count();
        assert_eq!(dailies, DAILY_KEEP);
        assert!(b.join("pre-migrate-1-to-0002.db").exists());
        assert!(b.join("daily-notes.txt").exists());
    }
}
