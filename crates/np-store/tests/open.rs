use np_store::{DB_FILE, MIGRATOR, Store};

fn latest() -> i64 {
    MIGRATOR.iter().map(|m| m.version).max().unwrap()
}

#[tokio::test]
async fn fresh_database_is_created_and_migrated_without_backup() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    let (store, report) = Store::open(dir.path(), &backups).await.unwrap();

    assert!(report.created);
    assert!(report.backup.is_none());
    assert_eq!(store.schema_version().await.unwrap(), Some(latest()));
    assert!(dir.path().join(DB_FILE).exists());
    store.ping().await.unwrap();

    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(mode, "wal");
    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(fk, 1);
}

#[tokio::test]
async fn reopening_an_up_to_date_database_does_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    let (store, _) = Store::open(dir.path(), &backups).await.unwrap();
    store.close().await;

    let (_store, report) = Store::open(dir.path(), &backups).await.unwrap();
    assert!(!report.created);
    assert!(report.applied.is_empty());
    assert!(report.backup.is_none());
    assert!(!backups.exists());
}

#[tokio::test]
async fn pending_migration_on_existing_data_takes_a_backup_first() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    let (store, _) = Store::open(dir.path(), &backups).await.unwrap();
    // Data the migrations never touch, to check it survives in both copies.
    sqlx::query("CREATE TABLE keep_me (v TEXT)")
        .execute(store.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO keep_me VALUES ('kept')")
        .execute(store.pool())
        .await
        .unwrap();
    // Pretend the latest migration has not run yet by undoing it by hand.
    let undo: &[&str] = match latest() {
        15 => &[
            "DROP TABLE github_connections",
            "DROP TABLE webhook_deliveries",
            "DROP TABLE webhooks",
            "DROP INDEX projects_public_board",
            "ALTER TABLE projects DROP COLUMN public_board_token",
        ],
        22 => &[
            "DROP TABLE work_item_votes",
            "ALTER TABLE comments DROP COLUMN is_public",
            "ALTER TABLE projects DROP COLUMN board_votes",
            "ALTER TABLE projects DROP COLUMN board_reactions",
            "ALTER TABLE projects DROP COLUMN board_comments",
        ],
        n => panic!("add the reversal of migration {n} to this test"),
    };
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ?")
        .bind(latest())
        .execute(store.pool())
        .await
        .unwrap();
    for q in undo {
        sqlx::query(q).execute(store.pool()).await.unwrap();
    }
    store.close().await;

    let (store, report) = Store::open(dir.path(), &backups).await.unwrap();
    let backup = report.backup.expect("a backup before migrating");
    assert!(backup.starts_with(&backups));
    assert_eq!(report.applied, vec![latest()]);
    assert_eq!(store.schema_version().await.unwrap(), Some(latest()));
    let v: String = sqlx::query_scalar("SELECT v FROM keep_me")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(v, "kept");

    // The backup is a readable database that still has the data, from before the migration.
    let copy = sqlx::SqlitePool::connect(&format!("sqlite:{}?mode=ro", backup.display()))
        .await
        .unwrap();
    let v: String = sqlx::query_scalar("SELECT v FROM keep_me")
        .fetch_one(&copy)
        .await
        .unwrap();
    assert_eq!(v, "kept");
    let before: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&copy)
        .await
        .unwrap();
    assert_eq!(before, Some(latest() - 1));
}
