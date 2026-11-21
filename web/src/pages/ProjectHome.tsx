import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ChartColumn, Plus } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Link, Navigate, useNavigate, useParams, useSearchParams } from "react-router";
import { patch, post } from "../api/client";
import type { Project, WorkItem } from "../api/types";
import { ProjectBar, type Crumb } from "../components/PageHeader";
import { CreateItemDialog } from "../components/CreateItem";
import { PriorityIcon, StateIcon } from "../components/Props";
import { KanbanLayout, ListLayout } from "../components/Layouts";
import { CalendarLayout, GanttLayout, SpreadsheetLayout } from "../components/MoreLayouts";
import { SavedViews } from "../components/SavedViews";
import { Picker } from "../components/Picker";
import { DisplayMenu, ExpandableSearch, FilterRow, FiltersToggle, LayoutSwitcher } from "../components/ViewBar";
import { PRIORITIES } from "../api/types";
import { activeFilterCount, filtersToQuery, groupItems, useViewState, type Group } from "../lib/view";
import { ErrorAlert, FullPageSpinner } from "../components/ui";
import { WorkItemDetail } from "../components/WorkItemDetail";
import type { PropsCtx } from "../components/WorkItemProps";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { recordVisit } from "../lib/home";
import { pcan } from "../lib/perm";
import { useProjectCtx } from "../lib/projectCtx";
import { pq, projectPath, useProject } from "../lib/projects";
import { itemsPath, useCreateItem, useUpdateItem, useWorkItems, wiq } from "../lib/workItems";
import { ProjectIcon } from "./Projects";
import { useWs } from "./WorkspaceHome";

export function ProjectHomePage() {
  const { t } = useI18n();
  const ws = useWs();
  const { projectId = "" } = useParams();
  const project = useProject(ws.slug, projectId);
  const qc = useQueryClient();
  const join = useMutation({
    mutationFn: () => post<Project>(projectPath(ws.slug, projectId) + "/join"),
    onSuccess: (p) => {
      qc.setQueryData(pq.one(ws.slug, projectId), p);
      qc.invalidateQueries({ queryKey: pq.all(ws.slug) });
    },
  });
  const ctx = useProjectCtx(ws.slug, project.data);
  if (project.isLoading) return <FullPageSpinner />;
  if (!project.data) return <Navigate to={`/${ws.slug}/projects`} replace />;
  const p = project.data;
  if (!p.my_role) {
    return (
      <div className="page page-wide">
        <div className="page-head">
          <ProjectIcon p={p} size={28} />
          <h1>{p.name}</h1>
        </div>
        <div className="card empty">
          <p>{t("proj.notMember")}</p>
          <button className="btn btn-primary" onClick={() => join.mutate()} disabled={join.isPending}>{t("proj.join")}</button>
        </div>
      </div>
    );
  }
  if (!ctx) return <FullPageSpinner />;
  return <WorkItemsView ctx={ctx} />;
}

/** Narrows the work item view, e.g. to one cycle: an extra server filter, values new items
 * get, and what to show under the project bar. */
export interface Scope {
  key: string;
  query: string;
  defaults: Partial<WorkItem>;
  header: ReactNode;
  /** Crumbs after the section, e.g. the cycle's name. */
  crumbs?: Crumb[];
}

export { ProjectBar } from "../components/PageHeader";

export function WorkItemsView({ ctx, scope }: { ctx: PropsCtx; scope?: Scope }) {
  const { t } = useI18n();
  const ws = useWs();
  const p = ctx.project;
  const [view, setView] = useViewState(scope ? `${p.id}:${scope.key}` : p.id);
  useEffect(() => {
    if (!scope) recordVisit(ws.slug, "project", p.id);
  }, [ws.slug, p.id, scope]);
  const [q, setQ] = useState(view.filters.q);
  useEffect(() => {
    const h = setTimeout(() => setQ(view.filters.q), 250);
    return () => clearTimeout(h);
  }, [view.filters.q]);
  const query = [scope?.query, filtersToQuery({ ...view.filters, q })].filter(Boolean).join("&");
  const items = useWorkItems(ws.slug, p.id, query);
  const update = useUpdateItem(ws.slug, p.id);
  const [params, setParams] = useSearchParams();
  const peek = params.get("peek");
  const [creating, setCreating] = useState(() => params.get("new") === "1");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [filterRow, setFilterRow] = useState(false);
  useEffect(() => {
    if (params.get("new") === "1") {
      setCreating(true);
      const next = new URLSearchParams(params);
      next.delete("new");
      setParams(next, { replace: true });
    }
  }, [params, setParams]);
  const setPeek = (id: string | null) => {
    const next = new URLSearchParams(params);
    if (id) next.set("peek", id);
    else next.delete("peek");
    setParams(next, { replace: !id });
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (peek) setPeek(null);
        else if (selected.size) setSelected(new Set());
      }
      const typing = (e.target as HTMLElement).closest("input, textarea, [contenteditable=true], select, [role=dialog]");
      if (!typing && e.key === "c" && !e.metaKey && !e.ctrlKey && pcan.createWorkItem(p) && !p.archived_at) {
        e.preventDefault();
        setCreating(true);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  });

  const all = useMemo(() => (items.data ?? []).filter((w) => view.showSubItems || !w.parent_id), [items.data, view.showSubItems]);
  const groups = useMemo(() => groupItems(all, view.groupBy, ctx, view.orderBy, view.showEmptyGroups), [all, view.groupBy, view.orderBy, view.showEmptyGroups, ctx]);
  const onPatch = (id: string, body: Partial<WorkItem>) => update.mutate({ id, body });
  const quickAdd = (g: Group) => <QuickAdd ctx={ctx} group={g} defaults={scope?.defaults} />;
  const [createDefaults, setCreateDefaults] = useState<Partial<WorkItem> | undefined>(undefined);
  const onCreateIn = (g: Group) => {
    setCreateDefaults({ ...scope?.defaults, ...groupDefaults(g) });
    setCreating(true);
  };
  const filtered = activeFilterCount(view.filters) > 0 || !!q.trim();

  const nFilters = activeFilterCount(view.filters);
  return (
    <div className="proj-view" style={{ position: "relative" }}>
      <ProjectBar
        ctx={ctx}
        crumbs={scope?.crumbs}
        extra={items.isSuccess && !scope?.crumbs ? <span className="badge count-chip" title={t("wi.countTip")}>{all.length}</span> : null}
      >
        {scope ? null : <SavedViews ctx={ctx} view={view} setView={setView} />}
        <ExpandableSearch value={view.filters.q} onChange={(v) => setView({ filters: { ...view.filters, q: v } })} label={t("view.search")} />
        <LayoutSwitcher view={view} setView={setView} />
        <FiltersToggle open={filterRow} onToggle={() => setFilterRow((o) => !o)} count={nFilters} />
        <DisplayMenu view={view} setView={setView} />
        {pcan.viewWorkItems(p) && !scope ? (
          <Link to={`/${ws.slug}/projects/${p.id}/analytics`} className="btn">
            <ChartColumn />
            {t("ptab.analytics")}
          </Link>
        ) : null}
        {pcan.createWorkItem(p) && !p.archived_at ? (
          <button className="btn btn-primary" onClick={() => setCreating(true)} title={`${t("wi.new")} (C)`}>{t("wi.add")}</button>
        ) : null}
      </ProjectBar>
      {scope?.header ?? null}
      {filterRow || nFilters > 0 ? <FilterRow view={view} setView={setView} ctx={ctx} /> : null}
      {!ctx.editable ? <div className="alert alert-warning" style={{ margin: "var(--space-3) var(--space-4) 0" }}>{t("wi.readOnly")}</div> : null}
      <ErrorAlert message={update.isError ? errorText(update.error, t) : null} />
      {items.isSuccess && filtered && all.length === 0 ? <div className="empty">{t("view.noMatch")}</div> : null}
      {view.layout === "kanban" ? (
        <KanbanLayout groups={groups} ctx={ctx} view={view} onOpen={setPeek} activeId={peek} onPatch={onPatch} quickAdd={quickAdd} onCreateIn={onCreateIn} />
      ) : view.layout === "spreadsheet" ? (
        <SpreadsheetLayout items={all} ctx={ctx} view={view} onOpen={setPeek} activeId={peek} onPatch={onPatch} />
      ) : view.layout === "calendar" ? (
        <CalendarLayout items={all} ctx={ctx} view={view} onOpen={setPeek} activeId={peek} onPatch={onPatch} />
      ) : view.layout === "gantt" ? (
        <GanttLayout items={all} ctx={ctx} view={view} onOpen={setPeek} activeId={peek} onPatch={onPatch} />
      ) : (
        <ListLayout groups={groups} ctx={ctx} view={view} onOpen={setPeek} activeId={peek} onPatch={onPatch} quickAdd={quickAdd} onCreateIn={onCreateIn} selected={selected} setSelected={setSelected} />
      )}
      {selected.size && view.layout === "list" ? <BulkBar ctx={ctx} ids={[...selected]} onDone={() => setSelected(new Set())} /> : null}
      {peek ? (
        <div className="peek" role="dialog" aria-label={t("wi.properties")}>
          <WorkItemDetail slug={ws.slug} ctx={ctx} id={peek} peek onClose={() => setPeek(null)} />
        </div>
      ) : null}
      {creating ? <CreateItemDialog slug={ws.slug} open={creating} onClose={() => (setCreating(false), setCreateDefaults(undefined))} ctx={ctx} defaults={createDefaults ?? scope?.defaults} onCreated={(w) => setPeek(w.id)} /> : null}
    </div>
  );
}

function BulkBar({ ctx, ids, onDone }: { ctx: PropsCtx; ids: string[]; onDone: () => void }) {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const apply = async (body: Record<string, unknown>) => {
    try {
      await patch<WorkItem[]>(itemsPath(ws.slug, ctx.project.id), { ids, ...body });
      qc.invalidateQueries({ queryKey: wiq.allLists(ws.slug, ctx.project.id) });
    } catch (e) {
      setError(errorText(e, t));
    }
  };
  const archive = async () => {
    for (const id of ids) await post(itemsPath(ws.slug, ctx.project.id, id) + "/archive").catch((e) => setError(errorText(e, t)));
    qc.invalidateQueries({ queryKey: wiq.allLists(ws.slug, ctx.project.id) });
    onDone();
  };
  return (
    <div className="bulk-bar" role="toolbar" aria-label={t("bulk.selected", { n: ids.length })}>
      <b className="small">{t("bulk.selected", { n: ids.length })}</b>
      <Picker label={t("wi.state")} className="btn btn-sm" value={[]} onChange={(v) => v[0] && apply({ state_id: v[0] })}
        options={ctx.states.map((s) => ({ value: s.id, label: s.name, icon: <StateIcon group={s.group} color={s.color} /> }))} trigger={t("wi.state")} />
      <Picker label={t("wi.priority")} className="btn btn-sm" searchable={false} value={[]} onChange={(v) => v[0] && apply({ priority: v[0] })}
        options={PRIORITIES.map((pr) => ({ value: pr, label: t(`prio.${pr}`), icon: <PriorityIcon p={pr} /> }))} trigger={t("wi.priority")} />
      <Picker label={t("wi.assignees")} className="btn btn-sm" value={[]} onChange={(v) => apply({ assignee_ids: v })} allowClear={t("wi.noAssignee")}
        options={ctx.members.map((m) => ({ value: m.user_id, label: m.name || m.email }))} trigger={t("wi.assignees")} />
      <Picker label={t("wi.labels")} className="btn btn-sm" value={[]} onChange={(v) => apply({ label_ids: v })} allowClear={t("wi.noLabels")}
        options={ctx.labels.map((l) => ({ value: l.id, label: l.name }))} trigger={t("wi.labels")} />
      <label className="btn btn-sm" style={{ position: "relative" }}>
        {t("wi.due")}
        <input type="date" className="date-overlay" aria-label={t("wi.due")} onChange={(e) => apply({ due_date: e.target.value || null })} />
      </label>
      <button className="btn btn-sm" onClick={archive}>{t("bulk.archive")}</button>
      <button className="btn btn-sm btn-ghost" onClick={onDone}>{t("bulk.clear")}</button>
      {error ? <span className="small" style={{ color: "var(--danger)" }}>{error}</span> : null}
    </div>
  );
}

/** Values a new item gets when created in a group. */
function groupDefaults(group: Group): Partial<WorkItem> {
  const b: Partial<WorkItem> = {};
  if (group.kind === "state") b.state_id = group.key;
  if (group.kind === "priority") b.priority = group.key as WorkItem["priority"];
  if (group.kind === "assignee" && group.key !== "none") b.assignee_ids = [group.key];
  if (group.kind === "label" && group.key !== "none") b.label_ids = [group.key];
  return b;
}

function QuickAdd({ ctx, group, defaults }: { ctx: PropsCtx; group: Group; defaults?: Partial<WorkItem> }) {
  const { t } = useI18n();
  const ws = useWs();
  const create = useCreateItem(ws.slug, ctx.project.id);
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  if (!open) {
    return (
      <button className="list-add" onClick={() => setOpen(true)} data-group={group.state?.name ?? group.key}>
        <Plus aria-hidden />
        {t("wi.quickAdd")}
      </button>
    );
  }
  const seed = () => {
    const b: Record<string, unknown> = { ...defaults, title };
    if (group.kind === "state") b.state_id = group.key;
    if (group.kind === "priority") b.priority = group.key;
    if (group.kind === "assignee" && group.key !== "none") b.assignee_ids = [group.key];
    if (group.kind === "label" && group.key !== "none") b.label_ids = [group.key];
    return b;
  };
  return (
    <form
      className="list-row list-row-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (!title.trim()) return;
        // Clear at once so the next title can be typed while this one saves.
        const body = seed();
        setTitle("");
        create.mutate(body, { onError: () => setTitle((cur) => cur || String(body.title)) });
      }}
    >
      <span className="list-key faint">{ctx.project.identifier}</span>
      <input
        className="inline-input"
        autoFocus
        aria-label={t("wi.title")}
        placeholder={t("wi.quickAddPlaceholder")}
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => e.key === "Escape" && (setOpen(false), setTitle(""))}
        onBlur={() => !title && setOpen(false)}
      />
      <span className="quick-hint">{t("wi.quickAddHint")}</span>
      {create.isError ? <span className="small" style={{ color: "var(--danger)" }}>{errorText(create.error, t)}</span> : null}
    </form>
  );
}

/** `/:slug/projects/:projectId/work-items/:itemId` */
export function WorkItemPage() {
  const ws = useWs();
  const { projectId = "", itemId = "" } = useParams();
  const project = useProject(ws.slug, projectId);
  const ctx = useProjectCtx(ws.slug, project.data);
  const navigate = useNavigate();
  if (project.isLoading) return <FullPageSpinner />;
  if (!project.data) return <Navigate to={`/${ws.slug}/projects`} replace />;
  if (!project.data.my_role) return <Navigate to={`/${ws.slug}/projects/${projectId}`} replace />;
  if (!ctx) return <FullPageSpinner />;
  return <WorkItemDetail slug={ws.slug} ctx={ctx} id={itemId} onClose={() => navigate(`/${ws.slug}/projects/${projectId}`)} />;
}
