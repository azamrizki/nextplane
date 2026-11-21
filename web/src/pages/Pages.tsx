// Project pages: a tree on the left, the live editor on the right (SPRINT S3.1).
import { recordVisit } from "../lib/home";
import { Lock } from "lucide-react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { Navigate, NavLink, useNavigate, useParams } from "react-router";
import { ApiError, del, get, patch, post } from "../api/client";
import type { Page, PageVersion, WorkItem } from "../api/types";
import { Avatar } from "../components/Avatar";
import { useEditorUpload } from "../components/Collab";
import { CollabEditor, type CollabHandle, type Presence } from "../components/CollabEditor";
import { ConfirmDialog, Dialog } from "../components/Dialog";
import { RefHover } from "../components/RefHover";
import { ErrorAlert, FullPageSpinner } from "../components/ui";
import type { PropsCtx } from "../components/WorkItemProps";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { flattenTree, isWithin, pagesPath, pgq, planMove, selectionTitle, usePage, usePageVersions, usePages, type DropZone } from "../lib/pages";
import { pcan } from "../lib/perm";
import { useProjectCtx } from "../lib/projectCtx";
import { projectPath, useProject, useProjects } from "../lib/projects";
import { useMe } from "../lib/session";
import { ProjectBar } from "./ProjectHome";
import { useWs } from "./WorkspaceHome";

function useCtx(): PropsCtx | null | "missing" {
  const ws = useWs();
  const { projectId = "" } = useParams();
  const project = useProject(ws.slug, projectId);
  const ctx = useProjectCtx(ws.slug, project.data);
  if (project.isSuccess && (!project.data?.my_role || !project.data.feature_pages)) return "missing";
  return ctx;
}

export function PagesPage() {
  const { t } = useI18n();
  const ws = useWs();
  const ctx = useCtx();
  const { pageId = "" } = useParams();
  const nav = useNavigate();
  const qc = useQueryClient();
  const [archived, setArchived] = useState(false);
  const p = ctx && ctx !== "missing" ? ctx.project : null;
  const list = usePages(ws.slug, p?.id ?? "", archived, !!p);
  useEffect(() => {
    if (p && pageId) recordVisit(ws.slug, "page", pageId, p.id);
  }, [ws.slug, p, pageId]);
  const create = useMutation({
    mutationFn: (parent: string | null) => post<Page>(pagesPath(ws.slug, p!.id), { title: "", parent_id: parent }),
    onSuccess: (pg) => {
      qc.invalidateQueries({ queryKey: pgq.all(ws.slug, p!.id) });
      setArchived(false);
      nav(`/${ws.slug}/projects/${p!.id}/pages/${pg.id}`, { state: { fresh: true } });
    },
  });
  const [drag, setDrag] = useState<string | null>(null);
  const [over, setOver] = useState<{ id: string; zone: DropZone } | null>(null);
  const move = useMutation({
    mutationFn: ({ id, body }: { id: string; body: { parent_id: string | null; sort_order: number } }) => patch<Page>(pagesPath(ws.slug, p!.id, id), body),
    onSettled: () => qc.invalidateQueries({ queryKey: pgq.all(ws.slug, p!.id) }),
  });
  if (ctx === "missing") return <Navigate to={`/${ws.slug}/projects/${p?.id ?? ""}`} replace />;
  if (!ctx || !p) return <FullPageSpinner />;
  const canCreate = pcan.createWorkItem(p) && !p.archived_at;
  const tree = flattenTree(list.data ?? []);
  return (
    <div className="proj-view">
      <ProjectBar ctx={ctx}>
        {canCreate ? (
          <button className="btn btn-sm btn-primary" onClick={() => create.mutate(null)} disabled={create.isPending}>
            {t("pg.new")}
          </button>
        ) : null}
      </ProjectBar>
      <div className="pages-layout">
        <nav className="pages-side" aria-label={t("ptab.pages")}>
          <div className="seg" role="tablist">
            <button role="tab" aria-selected={!archived} className={!archived ? "on" : ""} onClick={() => setArchived(false)}>{t("proj.active")}</button>
            <button role="tab" aria-selected={archived} className={archived ? "on" : ""} onClick={() => setArchived(true)}>{t("proj.archived")}</button>
          </div>
          {create.error ? <ErrorAlert message={errorText(create.error, t)} /> : null}
          {move.error ? <ErrorAlert message={errorText(move.error, t)} /> : null}
          {list.isSuccess && tree.length === 0 ? <p className="faint small">{t("pg.empty")}</p> : null}
          <ul className="page-tree">
            {tree.map(([pg, depth]) => (
              <li
                key={pg.id}
                style={{ paddingInlineStart: depth * 14 }}
                data-page={pg.title}
                onDragOver={(e) => {
                  const pages = list.data ?? [];
                  if (!drag || archived || isWithin(pages, pg.id, drag)) return;
                  e.preventDefault();
                  const box = e.currentTarget.getBoundingClientRect();
                  const f = (e.clientY - box.top) / box.height;
                  const zone: DropZone = f < 0.3 ? "before" : f > 0.7 ? "after" : "inside";
                  if (over?.id !== pg.id || over.zone !== zone) setOver({ id: pg.id, zone });
                }}
                onDragLeave={() => setOver((o) => (o?.id === pg.id ? null : o))}
                onDrop={(e) => {
                  e.preventDefault();
                  const plan = drag && over ? planMove(list.data ?? [], drag, over.id, over.zone) : null;
                  if (drag && plan) move.mutate({ id: drag, body: plan });
                  setDrag(null);
                  setOver(null);
                }}
              >
                <NavLink
                  to={`/${ws.slug}/projects/${p.id}/pages/${pg.id}`}
                  draggable={pg.can_edit && !archived}
                  onDragStart={(e) => {
                    e.dataTransfer.effectAllowed = "move";
                    e.dataTransfer.setData("text/plain", pg.id);
                    setDrag(pg.id);
                  }}
                  onDragEnd={() => {
                    setDrag(null);
                    setOver(null);
                  }}
                  className={({ isActive }) =>
                    "page-link" + (isActive ? " on" : "") + (drag === pg.id ? " dragging" : "") + (over?.id === pg.id ? ` drop-${over.zone}` : "")
                  }
                >
                  <span className="page-link-title">{pg.title || t("pg.untitled")}</span>
                  {pg.access === "private" ? <span className="page-flag" title={t("pg.private")} aria-label={t("pg.private")}>◐</span> : null}
                  {pg.locked_at ? <span className="page-flag" title={t("pg.lock")} aria-label={t("pg.lock")}><Lock /></span> : null}
                </NavLink>
              </li>
            ))}
          </ul>
        </nav>
        <main className="pages-main">
          {pageId ? (
            <PageView key={pageId} ctx={ctx} pageId={pageId} pages={list.data ?? []} onNewSub={canCreate ? () => create.mutate(pageId) : undefined} />
          ) : (
            <div className="card empty">{tree.length ? t("pg.select") : t("pg.empty")}</div>
          )}
        </main>
      </div>
    </div>
  );
}

function PageView({ ctx, pageId, pages, onNewSub }: { ctx: PropsCtx; pageId: string; pages: Page[]; onNewSub?: () => void }) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const me = useMe().data!;
  const nav = useNavigate();
  const qc = useQueryClient();
  const pid = ctx.project.id;
  const q = usePage(ws.slug, pid, pageId);
  const upload = useEditorUpload(ws.slug, pid);
  const [presence, setPresence] = useState<Presence[]>([]);
  const [showVersions, setShowVersions] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [moving, setMoving] = useState(false);
  const docRef = useRef<HTMLDivElement>(null);
  const projects = useProjects(ws.slug);
  const identifiers = (projects.data ?? []).map((x) => x.identifier);
  const canCreateItem = pcan.createWorkItem(ctx.project) && !ctx.project.archived_at;
  const duplicate = useMutation({
    mutationFn: () => post<Page>(`${url}/duplicate`, {}),
    onSuccess: (copy) => {
      qc.invalidateQueries({ queryKey: pgq.all(ws.slug, pid) });
      nav(`/${ws.slug}/projects/${pid}/pages/${copy.id}`);
    },
  });
  const createItem = async (text: string) => {
    const item = await post<WorkItem>(`${projectPath(ws.slug, pid)}/work-items`, { title: selectionTitle(text) });
    qc.invalidateQueries({ queryKey: ["work-items", ws.slug, pid] });
    return `${ctx.project.identifier}-${item.sequence}`;
  };
  const [note, setNote] = useState("");
  const editor = useRef<CollabHandle>(null);
  const url = pagesPath(ws.slug, pid, pageId);
  const refresh = (pg?: Page) => {
    if (pg) qc.setQueryData(pgq.one(ws.slug, pid, pageId), (old: Page | undefined) => ({ ...old, ...pg }));
    qc.invalidateQueries({ queryKey: pgq.all(ws.slug, pid) });
  };
  const save = useMutation({ mutationFn: (body: Partial<Page>) => patch<Page>(url, body), onSuccess: (pg) => refresh(pg) });
  const act = useMutation({ mutationFn: (a: string) => post<Page>(`${url}/do/${a}`, {}), onSuccess: (pg) => refresh(pg) });
  const remove = useMutation({
    mutationFn: () => del(url),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: pgq.all(ws.slug, pid) });
      nav(`/${ws.slug}/projects/${pid}/pages`);
    },
  });

  if (q.isLoading) return <FullPageSpinner />;
  if (!q.data) return <div className="card empty">{q.error instanceof ApiError && q.error.status !== 404 ? errorText(q.error, t) : t("pg.notFound")}</div>;
  const pg = q.data;
  const owner = ctx.members.find((m) => m.user_id === pg.owner_id);
  const people = dedupe(presence);
  const err = save.error ?? act.error ?? remove.error ?? duplicate.error;
  return (
    <article className="page-view">
      <div className="page-toolbar">
        <div className="presence" aria-label={t("pg.here")}>
          {people.map((u) => (
            <span key={u.name + u.color} className="presence-dot" style={{ borderColor: u.color }} title={u.name}>
              <Avatar name={u.name} size={22} />
            </span>
          ))}
          {people.length > 1 ? <span className="faint small">{t("pg.editing", { n: people.length })}</span> : null}
        </div>
        <div className="page-actions">
          <button className="btn btn-sm" onClick={() => setShowVersions((v) => !v)} aria-pressed={showVersions}>{t("pg.versions")}</button>
          {onNewSub && !pg.archived_at ? <button className="btn btn-sm" onClick={onNewSub}>{t("pg.newSub")}</button> : null}
          {onNewSub && !pg.archived_at ? <button className="btn btn-sm" onClick={() => duplicate.mutate()} disabled={duplicate.isPending}>{t("pg.duplicate")}</button> : null}
          {pg.can_edit ? <button className="btn btn-sm" onClick={() => setMoving(true)}>{t("pg.move")}</button> : null}
          {pg.owner_id === me.id && !pg.archived_at ? (
            <button className="btn btn-sm" onClick={() => save.mutate({ access: pg.access === "private" ? "project" : "private" })}>
              {pg.access === "private" ? t("pg.makePublic") : t("pg.makePrivate")}
            </button>
          ) : null}
          {pg.can_manage && !pg.archived_at ? <button className="btn btn-sm" onClick={() => act.mutate(pg.locked_at ? "unlock" : "lock")}>{pg.locked_at ? t("pg.unlock") : t("pg.lock")}</button> : null}
          {pg.can_manage ? <button className="btn btn-sm" onClick={() => act.mutate(pg.archived_at ? "unarchive" : "archive")}>{pg.archived_at ? t("pg.unarchive") : t("pg.archive")}</button> : null}
          {pg.can_manage ? <button className="btn btn-sm btn-ghost danger-text" onClick={() => setConfirmDelete(true)}>{t("pg.delete")}</button> : null}
        </div>
      </div>
      {err ? <ErrorAlert message={errorText(err, t)} /> : null}
      {note ? <div className="alert alert-success" role="status">{note}</div> : null}
      {pg.archived_at ? <div className="alert">{t("pg.archived")}</div> : pg.locked_at ? <div className="alert">{t("pg.locked")}</div> : !pg.can_edit ? <div className="faint small">{t("pg.readOnly")}</div> : null}
      {pg.access === "private" ? <div className="faint small">{t("pg.privateHint")}</div> : null}
      <div className={"page-body" + (showVersions ? " with-versions" : "")}>
        <div className="page-doc" ref={docRef}>
          <PageTitle page={pg} onSave={(title) => save.mutate({ title })} />
          <div className="faint small page-meta">
            {owner ? t("pg.by", { name: owner.name || owner.email }) : null} · {fmtDateTime(pg.updated_at, lang)}
          </div>
          <CollabEditor
            key={pg.id + ":" + pg.can_edit}
            pageId={pg.id}
            editable={pg.can_edit}
            me={{ id: me.id, name: me.name || me.email }}
            seedHtml={pg.has_doc ? undefined : pg.html}
            onPresence={setPresence}
            onUpload={upload}
            identifiers={identifiers}
            onCreateItem={canCreateItem ? createItem : undefined}
            handle={editor}
          />
          <RefHover slug={ws.slug} container={docRef} editable={pg.can_edit} />
        </div>
        {showVersions ? (
          <VersionPanel
            page={pg}
            nameOf={(id) => {
              const m = ctx.members.find((x) => x.user_id === id);
              return m ? m.name || m.email : "";
            }}
            onClose={() => setShowVersions(false)}
            onRestore={async (v) => {
              await post(`${url}/versions`, {});
              editor.current?.setContent(v.html ?? "");
              if (v.title !== pg.title) save.mutate({ title: v.title });
              qc.invalidateQueries({ queryKey: pgq.versions(ws.slug, pid, pageId) });
              setNote(t("pg.restoreDone"));
            }}
          />
        ) : null}
      </div>
      {moving ? <MoveDialog page={pg} pages={pages} onClose={() => setMoving(false)} onMove={(parent) => save.mutate({ parent_id: parent, sort_order: endOf(pages, parent) } as Partial<Page>)} /> : null}
      <ConfirmDialog
        open={confirmDelete}
        onClose={() => setConfirmDelete(false)}
        onConfirm={() => remove.mutate()}
        title={t("pg.delete")}
        body={t("pg.deleteBody")}
        confirmLabel={t("pg.delete")}
        danger
        busy={remove.isPending}
      />
    </article>
  );
}

function fmtDateTime(iso: string, lang: string): string {
  return new Date(iso).toLocaleString(lang === "id" ? "id-ID" : "en-US", { dateStyle: "medium", timeStyle: "short" });
}

/** A sort order after the last child of `parent`. */
function endOf(pages: Page[], parent: string | null): number {
  const kids = pages.filter((p) => (p.parent_id ?? null) === parent);
  return kids.length ? Math.max(...kids.map((k) => k.sort_order)) + 1000 : 1000;
}

function MoveDialog({ page, pages, onClose, onMove }: { page: Page; pages: Page[]; onClose: () => void; onMove: (parent: string | null) => void }) {
  const { t } = useI18n();
  const [parent, setParent] = useState(page.parent_id ?? "");
  const options = flattenTree(pages).filter(([p]) => !isWithin(pages, p.id, page.id));
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("pg.moveTitle", { name: page.title || t("pg.untitled") })}
      footer={
        <button
          className="btn btn-primary"
          onClick={() => {
            onMove(parent || null);
            onClose();
          }}
        >
          {t("pg.move")}
        </button>
      }
    >
      <p className="faint small">{t("pg.moveHint")}</p>
      <div className="field">
        <label htmlFor="page-parent">{t("pg.parent")}</label>
        <select id="page-parent" className="select" value={parent} onChange={(e) => setParent(e.target.value)}>
          <option value="">{t("pg.topLevel")}</option>
          {options.map(([p, depth]) => (
            <option key={p.id} value={p.id}>
              {"\u00a0\u00a0".repeat(depth) + (p.title || t("pg.untitled"))}
            </option>
          ))}
        </select>
      </div>
    </Dialog>
  );
}

/** One avatar per person (several tabs of the same person count once). */
function dedupe(list: Presence[]): Presence[] {
  const seen = new Map<string, Presence>();
  for (const p of list) seen.set(p.name + p.color, p);
  return [...seen.values()];
}

function PageTitle({ page, onSave }: { page: Page; onSave: (t: string) => void }) {
  const { t } = useI18n();
  const [value, setValue] = useState(page.title);
  const focused = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Someone else renamed it: follow, unless I am typing.
  useEffect(() => {
    if (!focused.current) setValue(page.title);
  }, [page.title]);
  // Leaving right after typing still saves the title.
  const latest = useRef({ value, title: page.title, onSave });
  latest.current = { value, title: page.title, onSave };
  useEffect(
    () => () => {
      if (!timer.current) return;
      clearTimeout(timer.current);
      const { value, title, onSave } = latest.current;
      if (value.trim() !== title) onSave(value.trim());
    },
    [],
  );
  const flush = (v: string) => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    if (v.trim() !== page.title) onSave(v.trim());
  };
  if (!page.can_edit) return <h1 className="page-title">{page.title || t("pg.untitled")}</h1>;
  return (
    <input
      className="page-title"
      value={value}
      placeholder={t("pg.titlePlaceholder")}
      aria-label={t("pg.titlePlaceholder")}
      autoFocus={!page.title}
      maxLength={255}
      onFocus={() => (focused.current = true)}
      onBlur={() => {
        focused.current = false;
        flush(value);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          (document.querySelector(".page-prose") as HTMLElement | null)?.focus();
        }
      }}
      onChange={(e) => {
        const v = e.target.value;
        setValue(v);
        if (timer.current) clearTimeout(timer.current);
        timer.current = setTimeout(() => flush(v), 800);
      }}
    />
  );
}

function VersionPanel({
  page,
  nameOf,
  onClose,
  onRestore,
}: {
  page: Page;
  nameOf: (id: string) => string;
  onClose: () => void;
  onRestore: (v: PageVersion) => Promise<void>;
}) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const pid = page.project_id;
  const list = usePageVersions(ws.slug, pid, page.id, true);
  const [open, setOpen] = useState<PageVersion | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const show = async (id: string) => {
    setError(null);
    try {
      setOpen(await get<PageVersion>(`${pagesPath(ws.slug, pid, page.id)}/versions/${id}`));
    } catch (e) {
      setError(e);
    }
  };
  return (
    <aside className="version-panel card" aria-label={t("pg.versions")}>
      <div className="row-flex">
        <h2>{t("pg.versions")}</h2>
        <button className="btn btn-sm btn-ghost" onClick={onClose}>{t("pg.close")}</button>
      </div>
      {error ? <ErrorAlert message={errorText(error, t)} /> : null}
      {page.can_edit ? (
        <button
          className="btn btn-sm"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            setError(null);
            try {
              await post(`${pagesPath(ws.slug, pid, page.id)}/versions`, {});
              await list.refetch();
            } catch (e) {
              setError(e);
            } finally {
              setBusy(false);
            }
          }}
        >
          {t("pg.saveVersion")}
        </button>
      ) : null}
      {list.isSuccess && list.data.length === 0 ? <p className="faint small">{t("pg.noVersions")}</p> : null}
      <ul className="version-list">
        {(list.data ?? []).map((v) => (
          <li key={v.id}>
            <button className={"version-item" + (open?.id === v.id ? " on" : "")} onClick={() => show(v.id)}>
              <span>{fmtDateTime(v.created_at, lang)}</span>
              <span className="faint small">
                {[v.created_by ? nameOf(v.created_by) : "", v.title || t("pg.untitled")].filter(Boolean).join(" · ")}
              </span>
            </button>
          </li>
        ))}
      </ul>
      {open ? (
        <div className="version-preview">
          {/* Stored HTML is sanitized on the server before it is saved. */}
          <div className="prose" dangerouslySetInnerHTML={{ __html: open.html ?? "" }} />
          {page.can_edit ? (
            <button
              className="btn btn-sm btn-primary"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                setError(null);
                try {
                  await onRestore(open);
                  setOpen(null);
                } catch (e) {
                  setError(e);
                } finally {
                  setBusy(false);
                }
              }}
            >
              {t("pg.restore")}
            </button>
          ) : null}
        </div>
      ) : null}
    </aside>
  );
}
