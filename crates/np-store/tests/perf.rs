//! 100k work items in one project (SPRINT S1.6). Run with
//! `cargo test -p np-store --release --test perf -- --ignored --nocapture`.

use std::time::Instant;

use np_core::work_item::Priority;
use np_store::Store;
use np_store::projects::NewProject;
use np_store::work_items::Filter;

const N: i64 = 100_000;
const BUDGET_MS: u128 = 200;
/// Analytics read the whole project, so they get a larger budget than a page of a list.
const ANALYTICS_MS: u128 = 1500;

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn hundred_thousand_items() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = Store::open(dir.path(), &dir.path().join("b"))
        .await
        .unwrap();
    let user = store
        .create_user("ana@example.com", "Ana", None, true)
        .await
        .unwrap();
    let ws = store
        .create_workspace("acme", "Acme", user.id)
        .await
        .unwrap();
    let pid = store
        .create_project(
            ws.workspace.id,
            &NewProject {
                identifier: "WEB".into(),
                name: "Web".into(),
                ..Default::default()
            },
            user.id,
        )
        .await
        .unwrap();
    let states = store.states(pid).await.unwrap();
    let label = store.create_label(pid, "Bug", "#f00", None).await.unwrap();

    // Seed directly in batches; the API path is covered elsewhere.
    let t0 = Instant::now();
    let pool = store.pool();
    let mut tx = pool.begin().await.unwrap();
    let prios = ["urgent", "high", "medium", "low", "none"];
    for i in 0..N {
        let id = np_core::id::new_id();
        let st = &states[(i % states.len() as i64) as usize];
        sqlx::query(
            "INSERT INTO work_items (id, project_id, sequence, title, description_html, state_id, priority, sort_order, created_by, created_at, updated_at, due_date)
             VALUES (?, ?, ?, ?, '', ?, ?, ?, ?, '2026-11-20T00:00:00.000000Z', '2026-11-20T00:00:00.000000Z', ?)",
        )
        .bind(id)
        .bind(pid)
        .bind(i + 1)
        .bind(format!("Work item number {i} about login flow {}", i % 97))
        .bind(st.id)
        .bind(prios[(i % 5) as usize])
        .bind(i as f64 * 10.0)
        .bind(user.id)
        .bind(if i % 3 == 0 { Some(format!("2026-12-{:02}", (i % 28) + 1)) } else { None })
        .execute(&mut *tx)
        .await
        .unwrap();
        if i % 7 == 0 {
            sqlx::query("INSERT INTO work_item_assignees (work_item_id, user_id) VALUES (?, ?)")
                .bind(id)
                .bind(user.id)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        if i % 11 == 0 {
            sqlx::query("INSERT INTO work_item_labels (work_item_id, label_id) VALUES (?, ?)")
                .bind(id)
                .bind(label.id)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        if i % 13 == 0 {
            sqlx::query("INSERT INTO search_fts (kind, ref_id, project_id, work_item_id, title, body) VALUES ('work_item', ?, ?, ?, ?, '')")
                .bind(id).bind(pid).bind(id).bind(format!("Work item number {i} about login flow")).execute(&mut *tx).await.unwrap();
        }
    }
    sqlx::query("UPDATE projects SET next_sequence = ? WHERE id = ?")
        .bind(N + 1)
        .bind(pid)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    sqlx::query("ANALYZE").execute(pool).await.unwrap();
    println!("seeded {N} items in {:?}", t0.elapsed());

    async fn page(store: &Store, pid: np_core::id::Uuid, f: Filter) -> (u128, usize) {
        let t = Instant::now();
        let n = store.work_items(pid, &f).await.unwrap().len();
        (t.elapsed().as_millis(), n)
    }
    let base = Filter {
        limit: Some(1000),
        ..Default::default()
    };
    let cases: Vec<(&str, Filter)> = vec![
        ("first page", base.clone()),
        (
            "page deep in the list",
            Filter {
                after: Some((90_000.0 * 10.0, 90_001)),
                ..base.clone()
            },
        ),
        (
            "by state",
            Filter {
                state_ids: vec![states[2].id],
                ..base.clone()
            },
        ),
        (
            "by priority",
            Filter {
                priorities: vec![Priority::Urgent, Priority::High],
                ..base.clone()
            },
        ),
        (
            "by assignee",
            Filter {
                assignee_ids: vec![user.id],
                ..base.clone()
            },
        ),
        (
            "unassigned",
            Filter {
                unassigned: true,
                ..base.clone()
            },
        ),
        (
            "by label",
            Filter {
                label_ids: vec![label.id],
                ..base.clone()
            },
        ),
        (
            "due range",
            Filter {
                due_after: Some("2026-12-01".into()),
                due_before: Some("2026-12-07".into()),
                ..base.clone()
            },
        ),
        (
            "title search",
            Filter {
                search: Some("number 4242".into()),
                ..base.clone()
            },
        ),
        (
            "state group done",
            Filter {
                state_groups: vec!["completed".into()],
                ..base.clone()
            },
        ),
    ];
    let mut worst = 0;
    for (name, f) in cases {
        let _ = page(&store, pid, f.clone()).await; // warm
        let (ms, n) = page(&store, pid, f).await;
        println!("{name:<24} {ms:>4} ms  ({n} rows)");
        worst = worst.max(ms);
    }
    let t = Instant::now();
    let hits = store.search(&[pid], "login flow", 20).await.unwrap();
    let fts = t.elapsed().as_millis();
    println!(
        "{:<24} {fts:>4} ms  ({} rows)",
        "full-text search",
        hits.len()
    );
    worst = worst.max(fts);
    // Analytics over the whole project.
    use np_store::analytics::{Dim, Measure};
    let t = Instant::now();
    store
        .overview(&[pid], 12, time::OffsetDateTime::now_utc())
        .await
        .unwrap();
    let ov = t.elapsed().as_millis();
    println!("{:<24} {ov:>4} ms", "analytics overview");
    let t = Instant::now();
    store
        .breakdown(&[pid], Dim::Label, Some(Dim::Priority), Measure::Count)
        .await
        .unwrap();
    let bd = t.elapsed().as_millis();
    println!("{:<24} {bd:>4} ms", "label × priority");
    println!(
        "analytics worst: {} ms (budget {ANALYTICS_MS} ms)",
        ov.max(bd)
    );
    assert!(
        ov.max(bd) < ANALYTICS_MS,
        "analytics took {} ms",
        ov.max(bd)
    );
    println!("worst: {worst} ms (budget {BUDGET_MS} ms)");
    assert!(worst < BUDGET_MS, "slowest query took {worst} ms");
}
