import { useQuery } from "@tanstack/react-query";
import { ArrowRight, Briefcase, CalendarClock, CircleAlert, House, ListTodo, Plus } from "lucide-react";
import { Link, useOutletContext } from "react-router";
import { get } from "../api/client";
import type { MyWorkspace, WorkItem } from "../api/types";
import { PageHeader } from "../components/PageHeader";
import { PriorityIcon, fmtDate, isOverdue } from "../components/Props";
import { useI18n } from "../i18n";
import { can } from "../lib/perm";
import { useProjects } from "../lib/projects";
import { useMe } from "../lib/session";
import { emitShell } from "../lib/shell";
import { ProjectIcon } from "./Projects";

export function useWs() {
  return useOutletContext<MyWorkspace>();
}

interface Row extends WorkItem {
  identifier: string;
  project_name: string;
}

/** Home, as in Plane: a greeting, then your open work and your projects. */
export function WorkspaceHomePage() {
  const { t, lang } = useI18n();
  const ws = useWs();
  const me = useMe().data!;
  const projects = useProjects(ws.slug);
  const mine = (projects.data ?? []).filter((p) => p.is_member && !p.archived_at);
  const assigned = useQuery({
    queryKey: ["my-work", ws.slug, "assigned", false],
    queryFn: () => get<Row[]>(`/workspaces/${ws.slug}/my-work?kind=assigned`),
  });
  const rows = assigned.data ?? [];
  const overdue = rows.filter((w) => isOverdue(w.due_date, w.completed_at)).length;
  const today = new Date();
  const week = new Date(today);
  week.setDate(week.getDate() + 7);
  const ymd = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  const dueSoon = rows.filter((w) => w.due_date && w.due_date >= ymd(today) && w.due_date <= ymd(week)).length;
  const hour = today.getHours();
  const greet = hour < 12 ? "home.morning" : hour < 18 ? "home.afternoon" : "home.evening";
  const first = (me.name || me.email).split(" ")[0] ?? "";
  return (
    <div className="page">
      <PageHeader
        crumbs={[{ icon: <House />, label: t("nav.home") }]}
        actions={
          ws.role !== "guest" ? (
            <button className="btn btn-primary" onClick={() => emitShell("create-item")}>
              {t("nav.newItem")}
            </button>
          ) : null
        }
      />
      <div className="home-hello">
        <h2>{t(greet, { name: first })}</h2>
        <span className="muted">
          {today.toLocaleDateString(lang === "id" ? "id-ID" : "en-US", { weekday: "long", month: "long", day: "numeric" })}
        </span>
      </div>
      <div className="home-stats">
        <Link to={`/${ws.slug}/my-work`} className="stat-card">
          <ListTodo aria-hidden />
          <span className="stat-card-value">{assigned.isSuccess ? rows.length : "—"}</span>
          <span className="stat-card-label">{t("home.assignedOpen")}</span>
        </Link>
        <Link to={`/${ws.slug}/my-work`} className="stat-card">
          <CircleAlert aria-hidden />
          <span className="stat-card-value">{assigned.isSuccess ? overdue : "—"}</span>
          <span className="stat-card-label">{t("home.overdue")}</span>
        </Link>
        <Link to={`/${ws.slug}/my-work`} className="stat-card">
          <CalendarClock aria-hidden />
          <span className="stat-card-value">{assigned.isSuccess ? dueSoon : "—"}</span>
          <span className="stat-card-label">{t("home.dueWeek")}</span>
        </Link>
      </div>
      <section className="home-section">
        <div className="home-section-head">
          <h3>{t("home.yourWork")}</h3>
          <Link to={`/${ws.slug}/my-work`} className="btn btn-sm btn-ghost">
            {t("home.viewAll")}
            <ArrowRight />
          </Link>
        </div>
        {assigned.isSuccess && rows.length === 0 ? <div className="card empty small">{t("home.nothingAssigned")}</div> : null}
        {rows.length ? (
          <div className="rows card">
            {rows.slice(0, 8).map((w) => (
              <Link key={w.id} to={`/${ws.slug}/projects/${w.project_id}/work-items/${w.id}`} className="row row-link">
                <PriorityIcon p={w.priority} />
                <span className="list-key" style={{ minWidth: 64 }}>{w.identifier}-{w.sequence}</span>
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
      </section>
      <section className="home-section">
        <div className="home-section-head">
          <h3>{t("home.projects")}</h3>
          <Link to={`/${ws.slug}/projects`} className="btn btn-sm btn-ghost">
            {t("home.viewAll")}
            <ArrowRight />
          </Link>
        </div>
        <div className="home-projects">
          {mine.map((p) => (
            <Link key={p.id} to={`/${ws.slug}/projects/${p.id}`} className="home-project">
              <span className="logo-tile">
                <ProjectIcon p={p} size={20} />
              </span>
              <span className="grow">
                <span className="home-project-name ellipsis">{p.name}</span>
                <span className="faint small">{p.identifier}</span>
              </span>
            </Link>
          ))}
          {projects.isSuccess && mine.length === 0 ? (
            <Link to={`/${ws.slug}/projects`} className="home-project faint">
              <span className="logo-tile">
                <Briefcase />
              </span>
              {t("nav.browseProjects")}
            </Link>
          ) : null}
          {can.createProject(ws) ? (
            <Link to={`/${ws.slug}/projects?new=1`} className="home-project home-project-add">
              <span className="logo-tile">
                <Plus />
              </span>
              {t("nav.createProject")}
            </Link>
          ) : null}
        </div>
      </section>
    </div>
  );
}
