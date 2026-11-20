// Analytics (SPRINT S3.4): an overview (tiles, weekly trend, breakdowns) and custom charts
// (any dimension, count or points, optionally segmented), for the workspace or one project.
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { Navigate, useParams } from "react-router";
import { get } from "../api/client";
import type { Priority, StateGroup } from "../api/types";
import { BarList, ChartCard, DataTable, StackedBars, StatTile, TrendLines, fmtNum, type Bar } from "../components/charts/Charts";
import { PriorityIcon, StateIcon } from "../components/Props";
import { FullPageSpinner } from "../components/ui";
import { useI18n } from "../i18n";
import { pcan } from "../lib/perm";
import { useProjectCtx } from "../lib/projectCtx";
import { useProject, useProjects } from "../lib/projects";
import { ProjectBar } from "./ProjectHome";
import { useWs } from "./WorkspaceHome";

interface Bucket {
  key: string;
  label: string;
  color: string | null;
  total: number;
}
interface Breakdown {
  x: Bucket[];
  segments: Bucket[];
  cells: { x: string; segment: string | null; value: number }[];
  total: number;
}
interface Overview {
  total: number;
  open: number;
  completed_30d: number;
  overdue: number;
  points_total: number;
  points_open: number;
  weeks: { start: string; created: number; completed: number }[];
  by_state_group: Breakdown;
  by_priority: Breakdown;
  by_assignee: Breakdown;
}

const DIMS = ["state_group", "state", "priority", "assignee", "label", "cycle", "module", "project"] as const;
type Dim = (typeof DIMS)[number];

/** Fixed color slots for ordered values: Urgent keeps its color whatever else is shown. */
const FIXED: Partial<Record<Dim, string[]>> = {
  priority: ["urgent", "high", "medium", "low", "none"],
  state_group: ["backlog", "unstarted", "started", "completed", "cancelled"],
};
const slotOf = (dim: Dim, key: string): number | undefined => {
  const i = FIXED[dim]?.indexOf(key) ?? -1;
  return i >= 0 ? i : undefined;
};

function useLabels() {
  const { t } = useI18n();
  return (dim: Dim, b: Bucket): { label: string; icon?: ReactNode } => {
    if (b.key === "__other") return { label: t("an.other") };
    if (dim === "state_group") return { label: t(`group.${b.key as StateGroup}`), icon: <StateIcon group={b.key as StateGroup} color={`var(--group-${b.key})`} /> };
    if (dim === "priority") return { label: t(`prio.${b.key as Priority}`), icon: <PriorityIcon p={b.key as Priority} /> };
    if (!b.key) {
      const none: Partial<Record<Dim, string>> = { assignee: t("an.unassigned"), label: t("an.noLabel"), cycle: t("an.noCycle"), module: t("an.noModule") };
      return { label: none[dim] ?? t("an.none") };
    }
    return { label: b.label };
  };
}

export function AnalyticsPage() {
  const { t } = useI18n();
  const ws = useWs();
  const projects = useProjects(ws.slug);
  const [project, setProject] = useState("");
  if (ws.role === "guest") return <Navigate to={`/${ws.slug}`} replace />;
  return (
    <div className="page page-wide">
      <div className="page-head">
        <h1>{t("an.title")}</h1>
      </div>
      <AnalyticsView
        project={project || undefined}
        filters={
          <div className="field">
            <label htmlFor="an-project">{t("an.project")}</label>
            <select id="an-project" className="select" value={project} onChange={(e) => setProject(e.target.value)}>
              <option value="">{t("an.allProjects")}</option>
              {(projects.data ?? []).filter((p) => pcan.viewWorkItems(p)).map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
          </div>
        }
      />
    </div>
  );
}

export function ProjectAnalyticsPage() {
  const ws = useWs();
  const { projectId = "" } = useParams();
  const project = useProject(ws.slug, projectId);
  const ctx = useProjectCtx(ws.slug, project.data);
  if (project.isSuccess && !pcan.viewWorkItems(project.data)) return <Navigate to={`/${ws.slug}/projects/${projectId}`} replace />;
  if (!ctx) return <FullPageSpinner />;
  return (
    <div className="proj-view">
      <ProjectBar ctx={ctx} />
      <div className="page page-wide">
        <AnalyticsView project={projectId} />
      </div>
    </div>
  );
}

function AnalyticsView({ project, filters }: { project?: string; filters?: ReactNode }) {
  const { t } = useI18n();
  const [tab, setTab] = useState<"overview" | "custom">("overview");
  return (
    <>
      <div className="an-filters">
        <div className="seg" role="tablist">
          {(["overview", "custom"] as const).map((v) => (
            <button key={v} role="tab" aria-selected={tab === v} className={tab === v ? "on" : ""} onClick={() => setTab(v)}>{t(`an.${v}`)}</button>
          ))}
        </div>
        {filters}
      </div>
      {tab === "overview" ? <OverviewTab project={project} /> : <CustomTab project={project} />}
    </>
  );
}

function OverviewTab({ project }: { project?: string }) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const label = useLabels();
  const q = useQuery({
    queryKey: ["analytics-overview", ws.slug, project ?? ""],
    queryFn: () => get<Overview>(`/workspaces/${ws.slug}/analytics/overview${project ? `?project=${project}` : ""}`),
    placeholderData: keepPreviousData,
  });
  if (!q.data) return <FullPageSpinner />;
  const o = q.data;
  const n = (v: number) => fmtNum(v, lang);
  const week = (s: string) => new Date(`${s}T00:00:00Z`).toLocaleDateString(lang === "id" ? "id-ID" : "en-US", { month: "short", day: "numeric", timeZone: "UTC" });
  const bars = (dim: Dim, b: Breakdown): Bar[] => b.x.map((x) => ({ key: x.key, value: x.total, ...label(dim, x) }));
  const breakdown = (title: string, dim: Dim, b: Breakdown) => {
    const rows = bars(dim, b);
    return (
      <ChartCard title={title} table={<DataTable head={[t(`an.dim.${dim}`), t("an.count")]} rows={rows.map((r) => [r.label, n(r.value)])} />}>
        <BarList rows={rows} total={b.total} empty={t("an.empty")} />
      </ChartCard>
    );
  };
  const created = t("an.created");
  const completed = t("an.completed");
  return (
    <div className={q.isPlaceholderData ? "refreshing" : ""}>
      <div className="stat-row">
        <StatTile label={t("an.open")} value={n(o.open)} hint={`${t("an.total")}: ${n(o.total)}`} />
        <StatTile label={t("an.completed30")} value={n(o.completed_30d)} />
        <StatTile label={t("an.overdue")} value={n(o.overdue)} />
        {o.points_total > 0 ? <StatTile label={t("an.openPoints")} value={n(o.points_open)} hint={t("an.pointsOf", { n: n(o.points_total) })} /> : null}
      </div>
      <div className="chart-grid">
        <ChartCard
          title={t("an.trend")}
          subtitle={t("an.trendSub")}
          table={<DataTable head={["", created, completed]} rows={o.weeks.map((w) => [t("an.week", { date: week(w.start) }), n(w.created), n(w.completed)])} />}
        >
          <TrendLines points={o.weeks.map((w) => ({ label: w.start, values: [w.created, w.completed] }))} series={[created, completed]} xLabel={(i) => week(o.weeks[i]!.start)} />
        </ChartCard>
        {breakdown(t("an.byStateGroup"), "state_group", o.by_state_group)}
        {breakdown(t("an.byPriority"), "priority", o.by_priority)}
        {breakdown(t("an.byAssignee"), "assignee", o.by_assignee)}
      </div>
      <p className="faint small">{t("an.liveHint")}</p>
    </div>
  );
}

function CustomTab({ project }: { project?: string }) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const label = useLabels();
  const [x, setX] = useState<Dim>(project ? "state" : "project");
  const [segment, setSegment] = useState<Dim | "">("priority");
  const [measure, setMeasure] = useState<"count" | "points">("count");
  const seg = segment === x ? "" : segment;
  const qs = new URLSearchParams({ x, measure, ...(seg ? { segment: seg } : {}), ...(project ? { project } : {}) });
  const q = useQuery({
    queryKey: ["analytics", ws.slug, project ?? "", x, seg, measure],
    queryFn: () => get<Breakdown>(`/workspaces/${ws.slug}/analytics?${qs}`),
    placeholderData: keepPreviousData,
  });
  const dims = DIMS.filter((d) => d !== "project" || !project);
  const n = (v: number) => fmtNum(v, lang);
  const b = q.data;
  const rows: Bar[] = (b?.x ?? []).map((xb) => ({ key: xb.key, value: xb.total, ...label(x, xb) }));
  const segments = seg ? (b?.segments ?? []).map((s) => ({ key: s.key, label: label(seg, s).label, slot: slotOf(seg, s.key) })) : [];
  const cells = new Map((b?.cells ?? []).map((c) => [`${c.x}\u0000${c.segment ?? ""}`, c.value]));
  const measureName = measure === "count" ? t("an.count") : t("an.points");
  const title = seg
    ? t("an.customSeg", { measure: measureName, dim: t(`an.dim.${x}`).toLowerCase(), seg: t(`an.dim.${seg}`).toLowerCase() })
    : t("an.customTitle", { measure: measureName, dim: t(`an.dim.${x}`).toLowerCase() });
  const table = seg ? (
    <DataTable head={[t(`an.dim.${x}`), ...segments.map((s) => s.label), t("an.total")]} rows={rows.map((r) => [r.label, ...segments.map((s) => n(cells.get(`${r.key}\u0000${s.key}`) ?? 0)), n(r.value)])} />
  ) : (
    <DataTable head={[t(`an.dim.${x}`), measureName]} rows={rows.map((r) => [r.label, n(r.value)])} />
  );
  return (
    <div className={q.isPlaceholderData ? "refreshing" : ""}>
      <div className="an-filters">
        <div className="field">
          <label htmlFor="an-x">{t("an.dimension")}</label>
          <select id="an-x" className="select" value={x} onChange={(e) => setX(e.target.value as Dim)}>
            {dims.map((d) => (
              <option key={d} value={d}>{t(`an.dim.${d}`)}</option>
            ))}
          </select>
        </div>
        <div className="field">
          <label htmlFor="an-seg">{t("an.segment")}</label>
          <select id="an-seg" className="select" value={seg} onChange={(e) => setSegment(e.target.value as Dim | "")}>
            <option value="">{t("an.none")}</option>
            {dims.filter((d) => d !== x).map((d) => (
              <option key={d} value={d}>{t(`an.dim.${d}`)}</option>
            ))}
          </select>
        </div>
        <div className="field">
          <label htmlFor="an-measure">{t("an.measure")}</label>
          <select id="an-measure" className="select" value={measure} onChange={(e) => setMeasure(e.target.value as "count" | "points")}>
            <option value="count">{t("an.count")}</option>
            <option value="points">{t("an.points")}</option>
          </select>
        </div>
      </div>
      {!b ? (
        <FullPageSpinner />
      ) : (
        <ChartCard title={title} subtitle={measure === "points" ? t("an.pointsHint") : undefined} table={table}>
          {seg ? <StackedBars rows={rows} segments={segments} cells={cells} empty={t("an.empty")} /> : <BarList rows={rows} total={b.total} empty={t("an.empty")} />}
        </ChartCard>
      )}
    </div>
  );
}
