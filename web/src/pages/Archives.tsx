import { useQueryClient } from "@tanstack/react-query";
import { Archive, ArchiveRestore, Boxes, Contrast, ListTodo, Settings2 } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Link, Navigate, useParams, useSearchParams } from "react-router";
import { post } from "../api/client";
import type { Cycle, Module, Project } from "../api/types";
import { ListLayout } from "../components/Layouts";
import { ProjectBar } from "../components/PageHeader";
import { DisplayMenu, ExpandableSearch, FilterRow, FiltersToggle } from "../components/ViewBar";
import { WorkItemDetail } from "../components/WorkItemDetail";
import type { PropsCtx } from "../components/WorkItemProps";
import { FullPageSpinner } from "../components/ui";
import { useI18n, type T } from "../i18n";
import { cq, cyclesPath, useCycles } from "../lib/cycles";
import { errorText } from "../lib/errors";
import { mq, modulesPath, useModules } from "../lib/modules";
import { pcan } from "../lib/perm";
import { useProjectCtx } from "../lib/projectCtx";
import { useProject } from "../lib/projects";
import { toast } from "../lib/toast";
import { EMPTY_FILTERS, activeFilterCount, filtersToQuery, groupItems, useViewState } from "../lib/view";
import { useWorkItems, wiq } from "../lib/workItems";
import { Meter, cycleDates } from "./Cycles";
import { ModuleStatusBadge } from "./Modules";
import { fmtDate } from "../components/Props";
import { useWs } from "./WorkspaceHome";

type Tab = "work-items" | "cycles" | "modules";

/** `/:slug/projects/:projectId/archives[/:tab]`: archived work items, cycles and modules, for
 * contributors and admins (as in Plane). */
export function ArchivesPage() {
  const ws = useWs();
  const { projectId = "", tab = "work-items" } = useParams();
  const project = useProject(ws.slug, projectId);
  const ctx = useProjectCtx(ws.slug, project.data);
  if (project.isLoading) return <FullPageSpinner />;
  const p = project.data;
  if (!p) return <Navigate to={`/${ws.slug}/projects`} replace />;
  if (!pcan.viewArchives(p)) return <Navigate to={`/${ws.slug}/projects/${p.id}`} replace />;
  const tabs = archiveTabs(p);
  if (!tabs.includes(tab as Tab)) return <Navigate to={`/${ws.slug}/projects/${p.id}/archives`} replace />;
  if (!ctx) return <FullPageSpinner />;
  return <Archives ctx={ctx} tab={tab as Tab} tabs={tabs} />;
}

function archiveTabs(p: Project): Tab[] {
  return ["work-items", ...(p.feature_cycles ? (["cycles"] as const) : []), ...(p.feature_modules ? (["modules"] as const) : [])];
}

const TAB_LABEL = { "work-items": "arch.tabItems", cycles: "arch.tabCycles", modules: "arch.tabModules" } as const;
const TAB_ICON: Record<Tab, ReactNode> = { "work-items": <ListTodo />, cycles: <Contrast />, modules: <Boxes /> };

function Archives({ ctx, tab, tabs }: { ctx: PropsCtx; tab: Tab; tabs: Tab[] }) {
  const { t } = useI18n();
  const ws = useWs();
  const base = `/${ws.slug}/projects/${ctx.project.id}/archives`;
  const strip = (
    <nav className="tabs archive-tabs" aria-label={t("arch.title")}>
      {tabs.map((x) => (
        <Link key={x} to={x === "work-items" ? base : `${base}/${x}`} className={x === tab ? "active" : ""} aria-current={x === tab ? "page" : undefined}>
          {TAB_ICON[x]}
          {t(TAB_LABEL[x])}
        </Link>
      ))}
    </nav>
  );
  if (tab === "cycles") return <ArchivedCycles ctx={ctx} strip={strip} />;
  if (tab === "modules") return <ArchivedModules ctx={ctx} strip={strip} />;
  return <ArchivedItems ctx={ctx} strip={strip} />;
}

/** Archived work items: the list layout only, read-only rows, the usual filters and display. */
function ArchivedItems({ ctx, strip }: { ctx: PropsCtx; strip: ReactNode }) {
  const { t } = useI18n();
  const ws = useWs();
  const p = ctx.project;
  const [view, setView] = useViewState(`${p.id}:archives`);
  const [q, setQ] = useState(view.filters.q);
  useEffect(() => {
    const h = setTimeout(() => setQ(view.filters.q), 250);
    return () => clearTimeout(h);
  }, [view.filters.q]);
  const query = ["archived=true", filtersToQuery({ ...view.filters, q })].filter(Boolean).join("&");
  const items = useWorkItems(ws.slug, p.id, query);
  const [params, setParams] = useSearchParams();
  const peek = params.get("peek");
  const [filterRow, setFilterRow] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const setPeek = (id: string | null) => {
    const next = new URLSearchParams(params);
    if (id) next.set("peek", id);
    else next.delete("peek");
    setParams(next, { replace: !id });
  };
  const readOnly = useMemo(() => ({ ...ctx, editable: false }), [ctx]);
  // Only closed items are archived, so empty groups (open states) would only be noise.
  const listView = useMemo(() => ({ ...view, layout: "list" as const, showSubItems: true, showEmptyGroups: false }), [view]);
  const all = items.data ?? [];
  const groups = useMemo(() => groupItems(all, view.groupBy, ctx, view.orderBy, false), [all, view.groupBy, view.orderBy, ctx]);
  const nFilters = activeFilterCount(view.filters);
  const filtered = nFilters > 0 || !!q.trim();
  return (
    <div className="proj-view" style={{ position: "relative" }}>
      <ProjectBar
        ctx={ctx}
        crumbs={[{ label: t("arch.tabItems") }]}
        extra={items.isSuccess ? <span className="badge count-chip" title={t("arch.countTip")}>{all.length}</span> : null}
      >
        <ExpandableSearch value={view.filters.q} onChange={(v) => setView({ filters: { ...view.filters, q: v } })} label={t("view.search")} />
        <FiltersToggle open={filterRow} onToggle={() => setFilterRow((o) => !o)} count={nFilters} />
        <DisplayMenu view={{ ...view, layout: "list" }} setView={setView} />
      </ProjectBar>
      {strip}
      {filterRow || nFilters > 0 ? <FilterRow view={view} setView={setView} ctx={ctx} /> : null}
      {items.isSuccess && all.length === 0 ? (
        filtered ? (
          <div className="empty">
            <p>{t("arch.noMatch")}</p>
            <button className="btn" onClick={() => setView({ filters: EMPTY_FILTERS })}>
              {t("view.clear")}
            </button>
          </div>
        ) : (
          <ArchiveEmpty title={t("arch.emptyItems")} body={t("arch.emptyItemsBody")}>
            {pcan.editSettings(p) ? (
              <Link className="btn btn-primary" to={`/${ws.slug}/projects/${p.id}/settings/automations`}>
                <Settings2 />
                {t("arch.setUpAutomation")}
              </Link>
            ) : null}
          </ArchiveEmpty>
        )
      ) : (
        <ListLayout groups={groups} ctx={readOnly} view={listView} onOpen={setPeek} activeId={peek} onPatch={() => {}} quickAdd={() => null} selected={selected} setSelected={setSelected} />
      )}
      {peek ? (
        <div className="peek" role="dialog" aria-label={t("wi.properties")}>
          <WorkItemDetail slug={ws.slug} ctx={ctx} id={peek} peek onClose={() => setPeek(null)} />
        </div>
      ) : null}
    </div>
  );
}

function ArchiveEmpty({ title, body, children }: { title: string; body: string; children?: ReactNode }) {
  return (
    <div className="archive-empty">
      <span className="archive-empty-icon" aria-hidden>
        <Archive />
      </span>
      <h2>{title}</h2>
      <p className="muted">{body}</p>
      {children}
    </div>
  );
}

function useRestore(ctx: PropsCtx, kind: "cycles" | "modules") {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const pid = ctx.project.id;
  return async (id: string) => {
    try {
      await post((kind === "cycles" ? cyclesPath : modulesPath)(ws.slug, pid, id) + "/unarchive");
      qc.invalidateQueries({ queryKey: (kind === "cycles" ? cq : mq).all(ws.slug, pid) });
      qc.invalidateQueries({ queryKey: [kind === "cycles" ? "cycle" : "module", ws.slug, pid] });
      qc.invalidateQueries({ queryKey: wiq.allLists(ws.slug, pid) });
      toast({ kind: "success", title: t(kind === "cycles" ? "arch.cycleRestored" : "arch.moduleRestored") });
    } catch (e) {
      toast({ kind: "error", title: errorText(e, t) });
    }
  };
}

function ArchivedCycles({ ctx, strip }: { ctx: PropsCtx; strip: ReactNode }) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const list = useCycles(ws.slug, ctx.project.id, true, true);
  const [q, setQ] = useState("");
  const restore = useRestore(ctx, "cycles");
  const shown = (list.data ?? []).filter((c) => c.name.toLowerCase().includes(q.trim().toLowerCase()));
  return (
    <div className="proj-view">
      <ProjectBar ctx={ctx} crumbs={[{ label: t("arch.tabCycles") }]}>
        <ExpandableSearch value={q} onChange={setQ} label={t("arch.searchCycles")} />
      </ProjectBar>
      {strip}
      <div className="page page-wide">
        {list.isSuccess && !list.data.length ? <ArchiveEmpty title={t("arch.emptyCycles")} body={t("arch.emptyCyclesBody")} /> : null}
        {list.isSuccess && list.data.length > 0 && !shown.length ? <div className="card empty small">{t("arch.noMatch")}</div> : null}
        {shown.length ? (
          <div className="rows card">
            {shown.map((c: Cycle) => (
              <div key={c.id} className="row cycle-row">
                <Link to={`/${ws.slug}/projects/${ctx.project.id}/cycles/${c.id}`} className="row-main row-link-inner">
                  <span className="cycle-name">{c.name}</span>
                  <span className="faint small">{cycleDates(c, lang, t)}</span>
                </Link>
                <div className="cycle-meter">
                  <Meter done={c.completed_items} total={c.total_items} />
                  <span className="faint small">{t("cyc.items", { done: c.completed_items, total: c.total_items })}</span>
                </div>
                {ctx.editable ? <RestoreButton t={t} onClick={() => restore(c.id)} /> : null}
              </div>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function ArchivedModules({ ctx, strip }: { ctx: PropsCtx; strip: ReactNode }) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const list = useModules(ws.slug, ctx.project.id, true, true);
  const [q, setQ] = useState("");
  const restore = useRestore(ctx, "modules");
  const shown = (list.data ?? []).filter((m) => m.name.toLowerCase().includes(q.trim().toLowerCase()));
  return (
    <div className="proj-view">
      <ProjectBar ctx={ctx} crumbs={[{ label: t("arch.tabModules") }]}>
        <ExpandableSearch value={q} onChange={setQ} label={t("arch.searchModules")} />
      </ProjectBar>
      {strip}
      <div className="page page-wide">
        {list.isSuccess && !list.data.length ? <ArchiveEmpty title={t("arch.emptyModules")} body={t("arch.emptyModulesBody")} /> : null}
        {list.isSuccess && list.data.length > 0 && !shown.length ? <div className="card empty small">{t("arch.noMatch")}</div> : null}
        {shown.length ? (
          <div className="rows card">
            {shown.map((m: Module) => (
              <div key={m.id} className="row cycle-row">
                <Link to={`/${ws.slug}/projects/${ctx.project.id}/modules/${m.id}`} className="row-main row-link-inner">
                  <span className="cycle-name">{m.name}</span>
                  <span className="faint small">{m.start_date && m.target_date ? `${fmtDate(m.start_date, lang)} – ${fmtDate(m.target_date, lang)}` : t("cyc.noDates")}</span>
                </Link>
                <ModuleStatusBadge status={m.status} />
                <div className="cycle-meter">
                  <Meter done={m.completed_items} total={m.total_items} />
                </div>
                {ctx.editable ? <RestoreButton t={t} onClick={() => restore(m.id)} /> : null}
              </div>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function RestoreButton({ t, onClick }: { t: T; onClick: () => void }) {
  return (
    <button className="btn btn-sm" onClick={onClick}>
      <ArchiveRestore />
      {t("wi.unarchive")}
    </button>
  );
}
