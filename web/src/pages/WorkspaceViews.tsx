import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Ellipsis, Layers, Link as LinkIcon, Pencil, Plus, Search, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, Navigate, useNavigate, useParams, useSearchParams } from "react-router";
import { del, getAll, patch, post } from "../api/client";
import type { Label, ProjMember, WorkItem } from "../api/types";
import { ConfirmDialog, Dialog } from "../components/Dialog";
import { Popover } from "../components/Layout";
import { SpreadsheetLayout } from "../components/MoreLayouts";
import { PageHeader } from "../components/PageHeader";
import { DisplayMenu, ExpandableSearch, FilterRow, FiltersToggle } from "../components/ViewBar";
import { WorkItemDetail } from "../components/WorkItemDetail";
import { ErrorAlert, FullPageSpinner, TextField } from "../components/ui";
import { useI18n, type T } from "../i18n";
import { copyText } from "../lib/copy";
import { errorText } from "../lib/errors";
import { useProjectCtxs } from "../lib/projectCtx";
import { useProjects } from "../lib/projects";
import { useMe } from "../lib/session";
import { emitShell } from "../lib/shell";
import { toast } from "../lib/toast";
import { DEFAULT_VIEW, EMPTY_FILTERS, activeFilterCount, filtersToQuery, useViewState, type ViewState } from "../lib/view";
import { useWs } from "./WorkspaceHome";

/** A workspace view as the server keeps it (a saved view without a project). */
export interface WsView {
  id: string;
  owner_id: string;
  name: string;
  description: string;
  is_shared: boolean;
  config: Partial<ViewState>;
}

type Row = WorkItem & { identifier: string; project_name: string };

const BUILTIN = [
  { id: "all", key: "wv.all" },
  { id: "assigned", key: "wv.assigned" },
  { id: "created", key: "wv.created" },
  { id: "subscribed", key: "wv.subscribed" },
] as const;
type BuiltinId = (typeof BUILTIN)[number]["id"];

const wvq = {
  views: (slug: string) => ["ws-views", slug] as const,
  items: (slug: string, q: string) => ["ws-items", slug, q] as const,
  allItems: (slug: string) => ["ws-items", slug] as const,
};

function useWsViews(slug: string) {
  return useQuery({ queryKey: wvq.views(slug), queryFn: () => getAll<WsView>(`/workspaces/${slug}/views`) });
}

/** A view's display state from what it stores (only what a spreadsheet uses). */
function stateOf(c: Partial<ViewState> | undefined): ViewState {
  return { ...DEFAULT_VIEW, ...c, layout: "spreadsheet", filters: { ...EMPTY_FILTERS, ...c?.filters }, display: { ...DEFAULT_VIEW.display, ...c?.display } };
}
const configOf = (v: ViewState): Partial<ViewState> => ({ filters: { ...v.filters, q: "" }, display: v.display, orderBy: v.orderBy, showSubItems: v.showSubItems });
const same = (a: Partial<ViewState>, b: Partial<ViewState>) => JSON.stringify(a) === JSON.stringify(b);

/** `/:slug/views` (the list) and `/:slug/views/:viewId` (one view, as a spreadsheet). */
export function WorkspaceViewsPage() {
  const { viewId } = useParams();
  return viewId ? <ViewPage key={viewId} viewId={viewId} /> : <ViewsList />;
}

function ViewsList() {
  const { t } = useI18n();
  const ws = useWs();
  const views = useWsViews(ws.slug);
  const [q, setQ] = useState("");
  const match = (s: string) => s.toLowerCase().includes(q.trim().toLowerCase());
  return (
    <div className="page page-wide">
      <PageHeader
        crumbs={[{ icon: <Layers />, label: t("nav.views") }]}
        actions={
          <Link to={`/${ws.slug}/views/all?new=1`} className="btn btn-primary">
            {t("wv.add")}
          </Link>
        }
      />
      <label className="search-box views-search">
        <Search aria-hidden />
        <input className="input" placeholder={t("wv.searchViews")} aria-label={t("wv.searchViews")} value={q} onChange={(e) => setQ(e.target.value)} />
      </label>
      <div className="rows card">
        {BUILTIN.filter((b) => match(t(b.key))).map((b) => (
          <Link key={b.id} to={`/${ws.slug}/views/${b.id}`} className="row row-link">
            <Layers className="inline-icon" aria-hidden />
            <span className="grow">{t(b.key)}</span>
            <span className="faint small">{t("wv.builtIn")}</span>
          </Link>
        ))}
        {(views.data ?? [])
          .filter((v) => match(v.name))
          .map((v) => (
            <div key={v.id} className="row">
              <Layers className="inline-icon" aria-hidden />
              <Link to={`/${ws.slug}/views/${v.id}`} className="grow row-link-inner">
                <span className="ellipsis" style={{ display: "block" }}>{v.name}</span>
                {v.description ? <span className="faint small ellipsis" style={{ display: "block" }}>{v.description}</span> : null}
              </Link>
              <ViewMenu view={v} />
            </div>
          ))}
      </div>
    </div>
  );
}

function ViewPage({ viewId }: { viewId: string }) {
  const { t } = useI18n();
  const ws = useWs();
  const me = useMe().data!;
  const qc = useQueryClient();
  const navigate = useNavigate();
  const views = useWsViews(ws.slug);
  const builtin = BUILTIN.find((b) => b.id === viewId);
  const custom = views.data?.find((v) => v.id === viewId);
  // Built-in views keep their display in this browser; custom ones start from what they store.
  const [local, setLocal] = useViewState(`ws:${ws.slug}:${builtin ? viewId : "custom"}`);
  const [draft, setDraft] = useState<ViewState | null>(null);
  useEffect(() => {
    if (custom) setDraft(stateOf(custom.config));
  }, [custom]);
  const view: ViewState = builtin ? { ...local, layout: "spreadsheet" } : (draft ?? stateOf(undefined));
  const setView = (p: Partial<ViewState>) => (builtin ? setLocal(p) : setDraft((d) => ({ ...(d ?? stateOf(undefined)), ...p })));
  const [q, setQ] = useState(view.filters.q);
  useEffect(() => {
    const h = setTimeout(() => setQ(view.filters.q), 250);
    return () => clearTimeout(h);
  }, [view.filters.q]);

  // The built-in views' own condition is added to the query, not shown as a filter.
  const implicit = (id: BuiltinId | undefined) => (id === "assigned" ? { assignee: [me.id] } : id === "created" ? { created_by: [me.id] } : {});
  const query = [filtersToQuery({ ...view.filters, ...implicit(builtin?.id), q }), builtin?.id === "subscribed" ? "subscribed=true" : ""].filter(Boolean).join("&");
  const items = useQuery({ queryKey: wvq.items(ws.slug, query), queryFn: () => getAll<Row>(`/workspaces/${ws.slug}/work-items${query ? `?${query}` : ""}`), enabled: !!builtin || !!custom });
  const projects = useProjects(ws.slug);
  const mine = useMemo(() => (projects.data ?? []).filter((p) => p.is_member && !p.archived_at), [projects.data]);
  const ctxs = useProjectCtxs(ws.slug, mine);
  const scope = useMemo(() => {
    const members = new Map<string, ProjMember>();
    const labels = new Map<string, Label>();
    for (const c of ctxs.values()) {
      c.members.forEach((m) => members.set(m.user_id, m));
      c.labels.forEach((l) => labels.set(l.id, l));
    }
    return { projects: mine, members: [...members.values()], labels: [...labels.values()] };
  }, [ctxs, mine]);
  const rows = useMemo(() => (items.data ?? []).filter((w) => view.showSubItems || !w.parent_id), [items.data, view.showSubItems]);

  const [params, setParams] = useSearchParams();
  const peek = params.get("peek");
  const peekItem = rows.find((w) => w.id === peek) ?? items.data?.find((w) => w.id === peek);
  const setPeek = (id: string | null) => {
    const next = new URLSearchParams(params);
    if (id) next.set("peek", id);
    else next.delete("peek");
    setParams(next, { replace: !id });
  };
  const [filterRow, setFilterRow] = useState(false);
  const [adding, setAdding] = useState(() => params.get("new") === "1");
  const [error, setError] = useState<string | null>(null);
  const onPatch = async (id: string, body: Partial<WorkItem>) => {
    const w = items.data?.find((x) => x.id === id);
    if (!w) return;
    qc.setQueryData<Row[]>(wvq.items(ws.slug, query), (old) => old?.map((x) => (x.id === id ? { ...x, ...body } : x)));
    try {
      await patch(`/workspaces/${ws.slug}/projects/${w.project_id}/work-items/${id}`, body);
      setError(null);
    } catch (e) {
      setError(errorText(e, t));
    }
    qc.invalidateQueries({ queryKey: wvq.allItems(ws.slug) });
    qc.invalidateQueries({ queryKey: ["work-items", ws.slug, w.project_id] });
  };
  const dirty = !!custom && !!draft && custom.owner_id === me.id && !same(configOf(draft), configOf(stateOf(custom.config)));
  const saveView = useMutation({
    mutationFn: () => patch<WsView>(`/workspaces/${ws.slug}/views/${custom!.id}`, { config: configOf(view) }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: wvq.views(ws.slug) });
      toast({ kind: "success", title: t("wv.saved") });
    },
  });

  if (!builtin && views.isLoading) return <FullPageSpinner />;
  const name = builtin ? t(builtin.key) : custom?.name;
  if (!builtin && !custom) {
    return (
      <div className="page">
        <PageHeader crumbs={[{ icon: <Layers />, label: t("nav.views"), to: `/${ws.slug}/views` }]} />
        <div className="archive-empty">
          <h2>{t("wv.missing")}</h2>
          <Link className="btn btn-primary" to={`/${ws.slug}/views/all`}>
            {t("wv.toAll")}
          </Link>
        </div>
      </div>
    );
  }
  const nFilters = activeFilterCount(view.filters);
  return (
    <div className="proj-view" style={{ position: "relative" }}>
      <PageHeader
        crumbs={[{ icon: <Layers />, label: t("nav.views"), to: `/${ws.slug}/views` }, { label: name }]}
        extra={items.isSuccess ? <span className="badge count-chip" title={t("wv.countTip")}>{rows.length}</span> : null}
        actions={
          <>
            <ExpandableSearch value={view.filters.q} onChange={(v) => setView({ filters: { ...view.filters, q: v } })} label={t("view.search")} />
            <FiltersToggle open={filterRow} onToggle={() => setFilterRow((o) => !o)} count={nFilters} />
            <DisplayMenu view={view} setView={setView} />
            <button className="btn btn-primary" onClick={() => setAdding(true)}>
              {t("wv.add")}
            </button>
            {custom ? <ViewMenu view={custom} onDeleted={() => navigate(`/${ws.slug}/views/all`)} /> : null}
          </>
        }
      />
      <nav className="tabs archive-tabs view-tabs" aria-label={t("nav.views")}>
        {BUILTIN.map((b) => (
          <Link key={b.id} to={`/${ws.slug}/views/${b.id}`} className={b.id === viewId ? "active" : ""} aria-current={b.id === viewId ? "page" : undefined}>
            {t(b.key)}
          </Link>
        ))}
        {(views.data ?? []).map((v) => (
          <Link key={v.id} to={`/${ws.slug}/views/${v.id}`} className={v.id === viewId ? "active" : ""} aria-current={v.id === viewId ? "page" : undefined}>
            {v.name}
          </Link>
        ))}
        <button className="icon-btn icon-btn-sm view-tabs-add" onClick={() => setAdding(true)} aria-label={t("wv.add")} title={t("wv.add")}>
          <Plus />
        </button>
      </nav>
      {filterRow || nFilters > 0 || dirty ? (
        <FilterRow
          view={view}
          setView={setView}
          scope={scope}
          extra={
            dirty ? (
              <button className="btn btn-sm btn-primary" onClick={() => saveView.mutate()} disabled={saveView.isPending}>
                {t("wv.save")}
              </button>
            ) : null
          }
        />
      ) : null}
      <ErrorAlert message={error ?? (saveView.isError ? errorText(saveView.error, t) : null)} />
      {projects.isSuccess && mine.length === 0 ? (
        <div className="archive-empty">
          <h2>{t("wv.noProjects")}</h2>
          <Link className="btn btn-primary" to={`/${ws.slug}/projects`}>
            {t("nav.browseProjects")}
          </Link>
        </div>
      ) : items.isSuccess && rows.length === 0 ? (
        <div className="archive-empty">
          <h2>{t("wv.empty")}</h2>
          {ws.role !== "guest" ? (
            <button className="btn btn-primary" onClick={() => emitShell("create-item")}>
              {t("nav.newItem")}
            </button>
          ) : null}
        </div>
      ) : (
        <SpreadsheetLayout items={rows} ctxOf={(w) => ctxs.get(w.project_id)} view={{ ...view, orderBy: view.orderBy === "manual" ? "created_at" : view.orderBy }} onOpen={setPeek} activeId={peek} onPatch={onPatch} />
      )}
      {peek && peekItem && ctxs.get(peekItem.project_id) ? (
        <div className="peek" role="dialog" aria-label={t("wi.properties")}>
          <WorkItemDetail slug={ws.slug} ctx={ctxs.get(peekItem.project_id)!} id={peek} peek onClose={() => setPeek(null)} />
        </div>
      ) : null}
      {adding ? (
        <ViewDialog
          title={t("wv.addTitle")}
          initial={{ name: name && custom ? `${name} 2` : "", description: "" }}
          submit={t("wv.create")}
          onClose={() => {
            setAdding(false);
            if (params.get("new")) {
              const next = new URLSearchParams(params);
              next.delete("new");
              setParams(next, { replace: true });
            }
          }}
          onSave={async (f) => {
            const v = await post<WsView>(`/workspaces/${ws.slug}/views`, { ...f, config: configOf({ ...view, filters: { ...view.filters, ...implicit(builtin?.id) } }) });
            await qc.invalidateQueries({ queryKey: wvq.views(ws.slug) });
            setAdding(false);
            navigate(`/${ws.slug}/views/${v.id}`);
          }}
        />
      ) : null}
    </div>
  );
}

function ViewDialog({ title, initial, submit, onClose, onSave }: { title: string; initial: { name: string; description: string }; submit: string; onClose: () => void; onSave: (f: { name: string; description: string }) => Promise<void> }) {
  const { t } = useI18n();
  const [f, setF] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const go = async () => {
    setBusy(true);
    try {
      await onSave({ name: f.name.trim(), description: f.description.trim() });
    } catch (e) {
      setError(errorText(e, t));
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={title}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button className="btn btn-primary" disabled={!f.name.trim() || busy} onClick={go}>
            {submit}
          </button>
        </>
      }
    >
      <ErrorAlert message={error} />
      <TextField label={t("wv.name")} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} autoFocus maxLength={80} />
      <TextField label={t("wv.description")} value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} maxLength={500} />
    </Dialog>
  );
}

/** "…" on a custom view: edit (its owner), copy link, delete (its owner or a workspace admin). */
function ViewMenu({ view, onDeleted }: { view: WsView; onDeleted?: () => void }) {
  const { t } = useI18n();
  const ws = useWs();
  const me = useMe().data!;
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const own = view.owner_id === me.id;
  const canDelete = own || ws.role === "owner" || ws.role === "admin";
  const url = `/workspaces/${ws.slug}/views/${view.id}`;
  return (
    <>
      <Popover
        open={open}
        onClose={close}
        right
        trigger={
          <button className="icon-btn bordered" onClick={() => setOpen(!open)} aria-label={t("wv.actions")} aria-haspopup="menu" aria-expanded={open}>
            <Ellipsis />
          </button>
        }
      >
        <div className="menu" role="menu">
          {own ? (
            <button className="menu-item" role="menuitem" onClick={() => (close(), setEditing(true))}>
              <Pencil />
              {t("common.edit")}
            </button>
          ) : null}
          <button className="menu-item" role="menuitem" onClick={() => (close(), void copyText(`${location.origin}/${ws.slug}/views/${view.id}`))}>
            <LinkIcon />
            {t("nav.copyLink")}
          </button>
          {canDelete ? (
            <button className="menu-item danger" role="menuitem" onClick={() => (close(), setDeleting(true))}>
              <Trash2 />
              {t("common.delete")}
            </button>
          ) : null}
        </div>
      </Popover>
      {editing ? (
        <ViewDialog
          title={t("wv.editTitle")}
          initial={{ name: view.name, description: view.description }}
          submit={t("common.save")}
          onClose={() => setEditing(false)}
          onSave={async (f) => {
            await patch(url, f);
            await qc.invalidateQueries({ queryKey: wvq.views(ws.slug) });
            setEditing(false);
          }}
        />
      ) : null}
      <ConfirmDialog
        open={deleting}
        onClose={() => setDeleting(false)}
        title={t("wv.delete")}
        body={t("wv.deleteBody", { name: view.name })}
        confirmLabel={t("common.delete")}
        danger
        onConfirm={async () => {
          setDeleting(false);
          try {
            await del(url);
            await qc.invalidateQueries({ queryKey: wvq.views(ws.slug) });
            toast({ kind: "success", title: t("wv.deleted") });
            onDeleted?.();
          } catch (e) {
            toast({ kind: "error", title: errorText(e, t as T) });
          }
        }}
      />
    </>
  );
}

/** `/:slug/views` without an id opens All work items, as Plane's sidebar link does. */
export function ViewsRedirect() {
  const ws = useWs();
  return <Navigate to={`/${ws.slug}/views/all`} replace />;
}
