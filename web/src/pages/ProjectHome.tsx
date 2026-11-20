import { useMutation, useQueryClient } from "@tanstack/react-query";
import { IntakeTab } from "../components/IntakeTab";
import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { Link, NavLink, Navigate, useNavigate, useParams, useSearchParams } from "react-router";
import { patch, post } from "../api/client";
import type { Project, WorkItem } from "../api/types";
import { Dialog } from "../components/Dialog";
import { Editor, type EditorValue } from "../components/Editor";
import { PriorityIcon, StateIcon } from "../components/Props";
import { KanbanLayout, ListLayout } from "../components/Layouts";
import { CalendarLayout, GanttLayout, SpreadsheetLayout } from "../components/MoreLayouts";
import { SavedViews } from "../components/SavedViews";
import { Picker } from "../components/Picker";
import { ViewBar } from "../components/ViewBar";
import { PRIORITIES } from "../api/types";
import { activeFilterCount, filtersToQuery, groupItems, useViewState, type Group } from "../lib/view";
import { ErrorAlert, FullPageSpinner } from "../components/ui";
import { WorkItemDetail } from "../components/WorkItemDetail";
import { AssigneesProp, CycleProp, ModulesProp, DateProp, EstimateProp, LabelsProp, PriorityProp, StateProp, type PropsCtx } from "../components/WorkItemProps";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
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
}

export function ProjectBar({ ctx, children }: { ctx: PropsCtx; children?: ReactNode }) {
  const { t } = useI18n();
  const ws = useWs();
  const p = ctx.project;
  const base = `/${ws.slug}/projects/${p.id}`;
  return (
    <div className="proj-bar">
      <ProjectIcon p={p} size={22} />
      <h1 className="proj-title">{p.name}</h1>
      <nav className="proj-tabs" aria-label={p.name}>
        <NavLink to={base} end className={({ isActive }) => (isActive ? "on" : "")}>{t("ptab.items")}</NavLink>
        {p.feature_cycles ? <NavLink to={`${base}/cycles`} className={({ isActive }) => (isActive ? "on" : "")}>{t("ptab.cycles")}</NavLink> : null}
        {p.feature_modules ? <NavLink to={`${base}/modules`} className={({ isActive }) => (isActive ? "on" : "")}>{t("ptab.modules")}</NavLink> : null}
        {p.feature_pages ? <NavLink to={`${base}/pages`} className={({ isActive }) => (isActive ? "on" : "")}>{t("ptab.pages")}</NavLink> : null}
        {p.feature_intake ? <IntakeTab slug={ws.slug} pid={p.id} to={`${base}/intake`} /> : null}
        {pcan.viewWorkItems(p) ? <NavLink to={`${base}/analytics`} className={({ isActive }) => (isActive ? "on" : "")}>{t("ptab.analytics")}</NavLink> : null}
      </nav>
      <span className="grow" />
      {children}
      <Link to={`${base}/settings`} className="btn btn-sm btn-ghost">{t("nav.settings")}</Link>
    </div>
  );
}

export function WorkItemsView({ ctx, scope }: { ctx: PropsCtx; scope?: Scope }) {
  const { t } = useI18n();
  const ws = useWs();
  const p = ctx.project;
  const [view, setView] = useViewState(scope ? `${p.id}:${scope.key}` : p.id);
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
  const filtered = activeFilterCount(view.filters) > 0 || !!q.trim();

  return (
    <div className="proj-view" style={{ position: "relative" }}>
      <ProjectBar ctx={ctx}>
        {pcan.createWorkItem(p) && !p.archived_at ? (
          <button className="btn btn-sm btn-primary" onClick={() => setCreating(true)} title="C">{t("wi.new")}</button>
        ) : null}
      </ProjectBar>
      {scope?.header ?? null}
      <ViewBar view={view} setView={setView} ctx={ctx} views={scope ? null : <SavedViews ctx={ctx} view={view} setView={setView} />} />
      {!ctx.editable ? <div className="alert alert-warning" style={{ margin: "var(--space-3) var(--space-4) 0" }}>{t("wi.readOnly")}</div> : null}
      <ErrorAlert message={update.isError ? errorText(update.error, t) : null} />
      {items.isSuccess && filtered && all.length === 0 ? <div className="empty">{t("view.noMatch")}</div> : null}
      {view.layout === "kanban" ? (
        <KanbanLayout groups={groups} ctx={ctx} view={view} onOpen={setPeek} activeId={peek} onPatch={onPatch} quickAdd={quickAdd} />
      ) : view.layout === "spreadsheet" ? (
        <SpreadsheetLayout items={all} ctx={ctx} view={view} onOpen={setPeek} activeId={peek} onPatch={onPatch} />
      ) : view.layout === "calendar" ? (
        <CalendarLayout items={all} ctx={ctx} view={view} onOpen={setPeek} activeId={peek} onPatch={onPatch} />
      ) : view.layout === "gantt" ? (
        <GanttLayout items={all} ctx={ctx} view={view} onOpen={setPeek} activeId={peek} onPatch={onPatch} />
      ) : (
        <ListLayout groups={groups} ctx={ctx} view={view} onOpen={setPeek} activeId={peek} onPatch={onPatch} quickAdd={quickAdd} selected={selected} setSelected={setSelected} />
      )}
      {selected.size && view.layout === "list" ? <BulkBar ctx={ctx} ids={[...selected]} onDone={() => setSelected(new Set())} /> : null}
      {peek ? (
        <div className="peek" role="dialog" aria-label={t("wi.properties")}>
          <WorkItemDetail slug={ws.slug} ctx={ctx} id={peek} peek onClose={() => setPeek(null)} />
        </div>
      ) : null}
      <CreateItemDialog open={creating} onClose={() => setCreating(false)} ctx={ctx} defaults={scope?.defaults} onCreated={(w) => setPeek(w.id)} />
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

function QuickAdd({ ctx, group, defaults }: { ctx: PropsCtx; group: Group; defaults?: Partial<WorkItem> }) {
  const { t } = useI18n();
  const ws = useWs();
  const create = useCreateItem(ws.slug, ctx.project.id);
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  if (!open) {
    return (
      <button className="list-add" onClick={() => setOpen(true)} data-group={group.state?.name ?? group.key}>
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
      {create.isError ? <span className="small" style={{ color: "var(--danger)" }}>{errorText(create.error, t)}</span> : null}
    </form>
  );
}

function CreateItemDialog({ open, onClose, ctx, onCreated, defaults }: { open: boolean; onClose: () => void; ctx: PropsCtx; onCreated: (w: WorkItem) => void; defaults?: Partial<WorkItem> }) {
  const { t } = useI18n();
  const ws = useWs();
  const create = useCreateItem(ws.slug, ctx.project.id);
  const blank = (): WorkItem => ({
    id: "", project_id: ctx.project.id, sequence: 0, title: "", state_id: ctx.states.find((s) => s.is_default)?.id ?? ctx.states[0]!.id,
    priority: "none", start_date: null, due_date: null, estimate: null, parent_id: null, sort_order: 0, created_by: "", created_at: "",
    updated_at: "", completed_at: null, archived_at: null, assignee_ids: ctx.project.default_assignee_id ? [ctx.project.default_assignee_id] : [], label_ids: [], sub_item_count: 0,
    cycle_id: null, module_ids: [], ...defaults,
  });
  const [draft, setDraft] = useState<WorkItem>(blank);
  const [desc, setDesc] = useState<EditorValue | null>(null);
  const [more, setMore] = useState(false);
  const [editorKey, setEditorKey] = useState(0);
  const onPatch = (b: Partial<WorkItem>) => setDraft((d) => ({ ...d, ...b }));
  const submit = (e: FormEvent) => {
    e.preventDefault();
    // Which button submitted: state set in its onClick would not be visible yet here.
    const asDraft = ((e.nativeEvent as SubmitEvent).submitter as HTMLButtonElement | null)?.dataset.draft === "1";
    create.mutate(
      {
        is_draft: asDraft,
        title: draft.title, state_id: draft.state_id, priority: draft.priority, assignee_ids: draft.assignee_ids, label_ids: draft.label_ids,
        start_date: draft.start_date, due_date: draft.due_date, estimate: draft.estimate, cycle_id: draft.cycle_id ?? null, module_ids: draft.module_ids ?? [],
        description_json: desc?.json ?? null, description_html: desc?.html ?? "",
      },
      {
        onSuccess: (w) => {
          setDraft(blank());
          setDesc(null);
          setEditorKey((k) => k + 1);
          if (!more) {
            onClose();
            if (!w.is_draft) onCreated(w);
          }
        },
      },
    );
  };
  return (
    <Dialog open={open} onClose={onClose} title={t("wi.new")} width={680}>
      <form className="form" onSubmit={submit}>
        <ErrorAlert message={create.isError ? errorText(create.error, t) : null} />
        <input className="wi-title" style={{ fontSize: "var(--text-xl)" }} autoFocus placeholder={t("wi.titlePlaceholder")} aria-label={t("wi.title")} value={draft.title} onChange={(e) => onPatch({ title: e.target.value })} maxLength={255} required />
        <div className="card">
          <Editor key={editorKey} placeholder={t("wi.descriptionPlaceholder")} ariaLabel={t("wi.description")} onChange={setDesc} debounce={150} minHeight={140} />
        </div>
        <div className="chip-row">
          <StateProp item={draft} ctx={ctx} onPatch={onPatch} />
          <PriorityProp item={draft} ctx={ctx} onPatch={onPatch} />
          <AssigneesProp item={draft} ctx={ctx} onPatch={onPatch} />
          <LabelsProp item={draft} ctx={ctx} onPatch={onPatch} />
          <DateProp item={draft} field="start_date" ctx={ctx} onPatch={onPatch} />
          <DateProp item={draft} field="due_date" ctx={ctx} onPatch={onPatch} />
          <EstimateProp item={draft} ctx={ctx} onPatch={onPatch} />
          <CycleProp item={draft} ctx={ctx} onPatch={onPatch} />
          <ModulesProp item={draft} ctx={ctx} onPatch={onPatch} />
        </div>
        <div className="row-flex">
          <label className="checkbox small">
            <input type="checkbox" checked={more} onChange={(e) => setMore(e.target.checked)} />
            {t("wi.createMore")}
          </label>
          <span className="row-flex" style={{ gap: "var(--space-2)" }}>
            <button className="btn" data-draft="1" disabled={create.isPending || !draft.title.trim()}>{t("draft.save")}</button>
            <button className="btn btn-primary" disabled={create.isPending || !draft.title.trim()}>{t("wi.create")}</button>
          </span>
        </div>
      </form>
    </Dialog>
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
