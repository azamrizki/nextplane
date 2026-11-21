import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { CalendarClock, ChevronDown, Download, UserRound } from "lucide-react";
import { useState } from "react";
import { Link, Navigate, useParams } from "react-router";
import { get, getAll } from "../api/client";
import { PRIORITIES, STATE_GROUPS, type Priority, type StateGroup, type WorkItem, type WsRole } from "../api/types";
import { Avatar } from "../components/Avatar";
import { BarList, ChartCard, DataTable, StatTile } from "../components/charts/Charts";
import { PageHeader } from "../components/PageHeader";
import { PriorityIcon, StateIcon, fmtDate, isOverdue } from "../components/Props";
import { timeAgo } from "../components/WorkItemDetail";
import { FullPageSpinner } from "../components/ui";
import { useI18n } from "../i18n";
import { useMe } from "../lib/session";
import { keyOf } from "../lib/workItems";
import { ProjectIcon } from "./Projects";
import { useWs } from "./WorkspaceHome";

interface Count {
  key: string;
  count: number;
}
interface Numbers {
  created: number;
  assigned: number;
  pending: number;
  completed: number;
}
interface ProfileData extends Numbers {
  user: { id: string; name: string; email: string | null; avatar: string | null; role: WsRole; joined_at: string };
  subscribed: number;
  by_state_group: Count[];
  by_priority: Count[];
  projects: (Numbers & { id: string; name: string; identifier: string; icon: string | null; cover_color: string | null })[] | null;
}
interface Entry {
  id: string;
  verb: string;
  field: string | null;
  old_value: string | null;
  new_value: string | null;
  created_at: string;
  work_item_id: string;
  project_id: string;
  sequence: number;
  title: string;
  identifier: string;
}
type Row = WorkItem & { identifier: string; project_name: string };

const TABS = ["summary", "assigned", "created", "subscribed", "activity"] as const;
type Tab = (typeof TABS)[number];

/** `/:slug/profile/:userId[/:tab]`: a member's summary, their work and their activity, counted
 * in the projects you belong to (as in Plane). "Your work" is your own profile. */
export function MemberProfilePage() {
  const { t } = useI18n();
  const ws = useWs();
  const me = useMe().data!;
  const { userId = "", tab = "summary" } = useParams();
  const profile = useQuery({ queryKey: ["profile", ws.slug, userId], queryFn: () => get<ProfileData>(`/workspaces/${ws.slug}/members/${userId}/profile`) });
  const guest = ws.role === "guest";
  if (!TABS.includes(tab as Tab) || (guest && tab !== "summary")) return <Navigate to={`/${ws.slug}/profile/${userId}`} replace />;
  if (profile.isLoading) return <FullPageSpinner />;
  if (!profile.data) return <div className="page"><div className="card empty">{t("prof.missing")}</div></div>;
  const p = profile.data;
  const self = userId === me.id;
  const base = `/${ws.slug}/profile/${userId}`;
  return (
    <div className="page page-wide profile-page">
      <PageHeader crumbs={[{ icon: <UserRound />, label: self ? t("nav.myWork") : p.user.name || p.user.email || "" }]} />
      <nav className="tabs profile-tabs" aria-label={t("prof.sections")}>
        {TABS.filter((x) => !guest || x === "summary").map((x) => (
          <Link key={x} to={x === "summary" ? base : `${base}/${x}`} className={x === tab ? "active" : ""} aria-current={x === tab ? "page" : undefined}>
            {t(`prof.tab.${x}`)}
          </Link>
        ))}
      </nav>
      <div className="profile-body">
        <div className="profile-main">
          {tab === "summary" ? <Summary p={p} base={base} /> : null}
          {tab === "assigned" || tab === "created" || tab === "subscribed" ? <Items userId={userId} kind={tab} /> : null}
          {tab === "activity" ? <Activity userId={userId} name={p.user.name || p.user.email || ""} /> : null}
        </div>
        <Sidebar p={p} />
      </div>
    </div>
  );
}

function Summary({ p, base }: { p: ProfileData; base: string }) {
  const { t, lang } = useI18n();
  const group = new Map(p.by_state_group.map((c) => [c.key, c.count]));
  const prio = new Map(p.by_priority.map((c) => [c.key, c.count]));
  const rows = PRIORITIES.map((k: Priority) => ({ key: k, label: t(`prio.${k}`), value: prio.get(k) ?? 0, icon: <PriorityIcon p={k} /> }));
  const n = (v: number) => new Intl.NumberFormat(lang === "id" ? "id-ID" : "en-US").format(v);
  return (
    <>
      <section className="profile-section">
        <h3 className="section-title">{t("prof.overview")}</h3>
        <div className="profile-tiles">
          {(["created", "assigned", "subscribed"] as const).map((k) => (
            <Link key={k} to={`${base}/${k}`} className="profile-tile-link">
              <StatTile label={t(`prof.tab.${k}`)} value={n(p[k])} />
            </Link>
          ))}
        </div>
      </section>
      <section className="profile-section">
        <h3 className="section-title">{t("prof.workload")}</h3>
        <div className="profile-tiles five">
          {STATE_GROUPS.map((g: StateGroup) => (
            <div key={g} className="stat-tile card workload-tile">
              <div className="stat-label">
                <StateIcon group={g} color={`var(--group-${g})`} />
                {t(`group.${g}`)}
              </div>
              <div className="stat-value">{n(group.get(g) ?? 0)}</div>
            </div>
          ))}
        </div>
      </section>
      <ChartCard title={t("prof.byPriority")} subtitle={t("prof.byPriorityHint")} table={<DataTable head={[t("wi.priority"), t("prof.items")]} rows={rows.map((r) => [r.label, r.value])} />}>
        <BarList rows={rows} total={p.assigned} empty={t("prof.noneAssigned")} />
      </ChartCard>
    </>
  );
}

function Items({ userId, kind }: { userId: string; kind: "assigned" | "created" | "subscribed" }) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const [closed, setClosed] = useState(false);
  const q = [kind === "assigned" ? `assignee=${userId}` : kind === "created" ? `created_by=${userId}` : `subscriber=${userId}`, closed ? "" : "state_group=backlog,unstarted,started"].filter(Boolean).join("&");
  const items = useQuery({ queryKey: ["ws-items", ws.slug, `profile:${q}`], queryFn: () => getAll<Row>(`/workspaces/${ws.slug}/work-items?${q}`) });
  const rows = items.data ?? [];
  return (
    <>
      <div className="profile-items-bar">
        <label className="switch-label">
          <input type="checkbox" role="switch" className="switch" checked={closed} onChange={(e) => setClosed(e.target.checked)} />
          {t("mw.closed")}
        </label>
      </div>
      {items.isSuccess && !rows.length ? <div className="card empty">{t("mw.empty")}</div> : null}
      {rows.length ? (
        <div className="rows card">
          {rows.map((w) => (
            <Link key={w.id} to={`/${ws.slug}/projects/${w.project_id}/work-items/${w.id}`} className="row row-link">
              <PriorityIcon p={w.priority} />
              <span className="mono faint small" style={{ width: 70 }}>{keyOf(w.identifier, w.sequence)}</span>
              <span className="grow ellipsis">{w.title}</span>
              <span className="faint small">{w.project_name}</span>
              {w.due_date ? (
                <span className={"small" + (isOverdue(w.due_date, w.completed_at) ? " overdue" : " faint")}>
                  <CalendarClock className="inline-icon" /> {fmtDate(w.due_date, lang)}
                </span>
              ) : null}
            </Link>
          ))}
        </div>
      ) : null}
    </>
  );
}

function Activity({ userId, name }: { userId: string; name: string }) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const url = `/workspaces/${ws.slug}/members/${userId}/activity`;
  const list = useInfiniteQuery({
    queryKey: ["profile-activity", ws.slug, userId],
    initialPageParam: "",
    queryFn: async ({ pageParam }) => {
      const res = await fetch(`/api/v1${url}?limit=100${pageParam ? `&cursor=${encodeURIComponent(pageParam)}` : ""}`, { credentials: "same-origin" });
      if (!res.ok) throw new Error(res.statusText);
      return { rows: (await res.json()) as Entry[], next: res.headers.get("x-next-cursor") };
    },
    getNextPageParam: (last) => last.next ?? undefined,
  });
  const rows = list.data?.pages.flatMap((p) => p.rows) ?? [];
  const d = new Date();
  const today = `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, "0")}-${String(d.getUTCDate()).padStart(2, "0")}`;
  const what = (e: Entry) => {
    if (e.verb === "created") return t("prof.act.created");
    if (e.verb === "archived") return t("prof.act.archived");
    if (e.verb === "restored") return t("prof.act.restored");
    const field = e.field ? t(`field.${e.field}` as "field.title") : "";
    return t("prof.act.changed", { field });
  };
  return (
    <>
      <div className="profile-items-bar">
        <a className="btn btn-sm" href={`/api/v1${url}?date=${today}&format=csv`} download>
          <Download />
          {t("prof.download")}
        </a>
      </div>
      {list.isSuccess && !rows.length ? <div className="card empty">{t("prof.noActivity")}</div> : null}
      {rows.length ? (
        <ol className="profile-activity card">
          {rows.map((e) => (
            <li key={e.id}>
              <span>
                <b>{name}</b> {what(e)}{" "}
                <Link to={`/${ws.slug}/projects/${e.project_id}/work-items/${e.work_item_id}`}>
                  <span className="mono">{keyOf(e.identifier, e.sequence)}</span> {e.title}
                </Link>
              </span>
              <span className="faint small">{timeAgo(e.created_at, t, lang)}</span>
            </li>
          ))}
        </ol>
      ) : null}
      {list.hasNextPage ? (
        <button className="btn widget-more" onClick={() => list.fetchNextPage()} disabled={list.isFetchingNextPage}>
          {t("st.loadMore")}
        </button>
      ) : null}
    </>
  );
}

function Sidebar({ p }: { p: ProfileData }) {
  const { t, lang } = useI18n();
  return (
    <aside className="profile-side card" aria-label={t("prof.about")}>
      <div className="profile-cover" aria-hidden />
      <div className="profile-who">
        <Avatar name={p.user.name} email={p.user.email ?? ""} src={p.user.avatar} size={64} />
        <h2>{p.user.name || p.user.email}</h2>
        {p.user.email && p.user.name ? <span className="faint small">{p.user.email}</span> : null}
      </div>
      <dl className="profile-facts">
        <dt>{t("prof.role")}</dt>
        <dd>{t(`role.${p.user.role}` as "role.member")}</dd>
        <dt>{t("prof.joined")}</dt>
        <dd>{fmtDate(p.user.joined_at.slice(0, 10), lang)}</dd>
      </dl>
      {p.projects?.length ? (
        <div className="profile-projects">
          {p.projects.map((x) => (
            <ProjectStats key={x.id} x={x} />
          ))}
        </div>
      ) : null}
    </aside>
  );
}

function ProjectStats({ x }: { x: NonNullable<ProfileData["projects"]>[number] }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const pct = x.assigned ? Math.round((x.completed / x.assigned) * 100) : null;
  return (
    <div className="profile-project">
      <button className="profile-project-head" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        <ProjectIcon p={x} size={18} />
        <span className="grow ellipsis">{x.name}</span>
        {pct !== null ? <span className={"badge " + (pct <= 35 ? "badge-danger" : pct <= 70 ? "badge-warning" : "badge-success")} title={t("prof.doneShare")}>{pct}%</span> : null}
        <ChevronDown className={open ? "rot180" : ""} />
      </button>
      {open ? (
        <dl className="profile-facts small">
          <dt>{t("prof.tab.created")}</dt>
          <dd>{x.created}</dd>
          <dt>{t("prof.tab.assigned")}</dt>
          <dd>{x.assigned}</dd>
          <dt>{t("prof.pending")}</dt>
          <dd>{x.pending}</dd>
          <dt>{t("group.completed")}</dt>
          <dd>{x.completed}</dd>
        </dl>
      ) : null}
    </div>
  );
}
