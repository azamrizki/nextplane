//! Project automations (S3.7, as in Plane): once a day the server archives work items that have
//! been closed for the project's `archive_in` months, and moves items nobody touched for
//! `close_in` months to the project's chosen cancelled state. A month is 30 days. The changes
//! are made by the workspace's "NextPlane" service account, so activity, notifications,
//! realtime screens and webhooks show them like any other change.

use np_core::id::Uuid;
use np_core::project::StateGroup;
use np_store::Store;
use np_store::automations::AutomatedProject;
use np_store::work_items::WorkItemPatch;
use time::OffsetDateTime;

use crate::events::{Bus, Event};

/// Name of the service account that makes automated changes.
pub const ACTOR: &str = "NextPlane";
/// Items one pass changes per project; the rest wait for the next day.
const BATCH: i64 = 500;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Ran {
    pub archived: usize,
    pub closed: usize,
}

/// One pass over every project with an automation. A project that fails is logged and
/// skipped; the others still run.
pub async fn run(store: &Store, events: &Bus, now: OffsetDateTime) -> anyhow::Result<Ran> {
    let mut total = Ran::default();
    for p in store.automated_projects().await? {
        match run_project(store, events, &p, now).await {
            Ok(r) => {
                total.archived += r.archived;
                total.closed += r.closed;
            }
            Err(e) => tracing::warn!(project = %p.id, error = %e, "project automation failed"),
        }
    }
    Ok(total)
}

fn day(t: OffsetDateTime) -> String {
    let d = t.date();
    format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())
}

async fn run_project(
    store: &Store,
    events: &Bus,
    p: &AutomatedProject,
    now: OffsetDateTime,
) -> np_core::Result<Ran> {
    let today = day(now);
    let cutoff = |months: i64| np_store::fmt_ts(now - time::Duration::days(30 * months));
    let mut ran = Ran::default();
    let mut bot: Option<Uuid> = None;
    let publish = |id: Uuid, action: &str, actor: Uuid| {
        events.publish(Event {
            workspace: p.slug.clone(),
            workspace_id: Some(p.workspace_id),
            project_id: Some(p.id),
            work_item_id: Some(id),
            area: "work_item".into(),
            method: "POST".into(),
            action: action.into(),
            actor: Some(actor),
            client: None,
        })
    };
    if p.archive_in > 0 {
        let ids = store
            .automation_candidates(
                p.id,
                &["completed", "cancelled"],
                &cutoff(p.archive_in),
                &today,
                BATCH,
            )
            .await?;
        for id in ids {
            let actor = match bot {
                Some(b) => b,
                None => *bot.insert(store.service_bot(p.workspace_id, ACTOR).await?),
            };
            store.set_work_item_archived(p.id, id, true, actor).await?;
            publish(id, "archived", actor);
            ran.archived += 1;
        }
    }
    if p.close_in > 0 {
        // The chosen state must still be one of this project's cancelled states.
        let target = match p.close_state_id {
            Some(s) => store
                .states(p.id)
                .await?
                .into_iter()
                .find(|x| x.id == s && x.group.0 == StateGroup::Cancelled)
                .map(|x| x.id),
            None => None,
        };
        let Some(target) = target else {
            tracing::warn!(project = %p.id, "auto-close has no cancelled state to move items to");
            return Ok(ran);
        };
        let ids = store
            .automation_candidates(
                p.id,
                &["backlog", "unstarted", "started"],
                &cutoff(p.close_in),
                &today,
                BATCH,
            )
            .await?;
        let patch = WorkItemPatch {
            state_id: Some(target),
            ..Default::default()
        };
        for id in ids {
            let actor = match bot {
                Some(b) => b,
                None => *bot.insert(store.service_bot(p.workspace_id, ACTOR).await?),
            };
            store.update_work_item(p.id, id, &patch, actor).await?;
            publish(id, "updated", actor);
            ran.closed += 1;
        }
    }
    Ok(ran)
}
