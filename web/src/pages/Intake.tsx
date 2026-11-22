// Project intake: requests waiting for triage (SPRINT S3.3).
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState, type FormEvent } from "react";
import { Link, Navigate, NavLink, useNavigate, useParams } from "react-router";
import { ApiError, del, get, patch, post } from "../api/client";
import type { IntakeEntry, IntakeStatus, WorkItem } from "../api/types";
import { Avatar } from "../components/Avatar";
import { Comments, useEditorUpload } from "../components/Collab";
import { ConfirmDialog, Dialog } from "../components/Dialog";
import { Editor, type EditorHandle } from "../components/Editor";
import { ErrorAlert, FullPageSpinner, TextField } from "../components/ui";
import type { PropsCtx } from "../components/WorkItemProps";
import { timeAgo } from "../components/WorkItemDetail";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { intakePath, iq, mayEditIntake, useIntake, useIntakeEntry, type IntakeViewName } from "../lib/intake";
import { projAtLeast } from "../lib/perm";
import { useProjectCtx } from "../lib/projectCtx";
import { projectPath, useProject } from "../lib/projects";
import { useMe } from "../lib/session";
import { itemsPath } from "../lib/workItems";
import { ProjectBar } from "./ProjectHome";
import { useWs } from "./WorkspaceHome";
import { DateField } from "../components/DatePicker";
import { addDays, todayIso } from "../lib/dates";

function useCtx(): PropsCtx | null | "missing" {
  const ws = useWs();
  const { projectId = "" } = useParams();
  const project = useProject(ws.slug, projectId);
  const ctx = useProjectCtx(ws.slug, project.data);
  if (project.isSuccess && (!project.data?.my_role || !project.data.feature_intake)) return "missing";
  return ctx;
}

export function StatusBadge({ status }: { status: IntakeStatus }) {
  const { t } = useI18n();
  return <span className={`intake-status st-${status}`}>{t(`in.st.${status}`)}</span>;
}

export function IntakePage() {
  const { t, lang } = useI18n();
  const ws = useWs();
  const ctx = useCtx();
  const { itemId = "" } = useParams();
  const [view, setView] = useState<IntakeViewName>("open");
  const [creating, setCreating] = useState(false);
  const [formOpen, setFormOpen] = useState(false);
  const p = ctx && ctx !== "missing" ? ctx.project : null;
  const list = useIntake(ws.slug, p?.id ?? "", view, !!p);
  if (ctx === "missing") return <Navigate to={`/${ws.slug}/projects/${p?.id ?? ""}`} replace />;
  if (!ctx || !p) return <FullPageSpinner />;
  const isAdmin = p.my_role === "admin";
  const rows = list.data ?? [];
  const nameOf = (e: IntakeEntry) => {
    if (e.source === "form") return e.submitter_name || e.submitter_email || t("in.someone");
    const m = ctx.members.find((x) => x.user_id === e.created_by);
    return m ? m.name || m.email : t("in.someone");
  };
  return (
    <div className="proj-view">
      <ProjectBar ctx={ctx}>
        {isAdmin ? <button className="btn btn-sm" onClick={() => setFormOpen(true)}>{t("in.formSettings")}</button> : null}
        {!p.archived_at ? <button className="btn btn-sm btn-primary" onClick={() => setCreating(true)}>{t("in.new")}</button> : null}
      </ProjectBar>
      <div className="pages-layout">
        <nav className="pages-side" aria-label={t("in.title")}>
          <div className="seg" role="tablist">
            {(["open", "snoozed", "closed"] as IntakeViewName[]).map((v) => (
              <button key={v} role="tab" aria-selected={view === v} className={view === v ? "on" : ""} onClick={() => setView(v)}>{t(`in.${v}`)}</button>
            ))}
          </div>
          {!projAtLeast(p.my_role, "commenter") ? <p className="faint small">{t("in.guestHint")}</p> : null}
          {list.isSuccess && rows.length === 0 ? <p className="faint small">{t("in.empty")}</p> : null}
          <ul className="intake-list">
            {rows.map((e) => (
              <li key={e.work_item_id}>
                <NavLink to={`/${ws.slug}/projects/${p.id}/intake/${e.work_item_id}`} className={({ isActive }) => "intake-row" + (isActive ? " on" : "")}>
                  <span className="intake-row-title">{e.title}</span>
                  <span className="faint small intake-row-meta">
                    {nameOf(e)} · {timeAgo(e.created_at, t, lang)}
                    {view !== "open" ? <StatusBadge status={e.status} /> : null}
                  </span>
                </NavLink>
              </li>
            ))}
          </ul>
        </nav>
        <main className="pages-main">
          {itemId ? <IntakeDetail key={itemId} ctx={ctx} id={itemId} nameOf={nameOf} /> : <div className="card empty">{rows.length ? t("in.select") : t("in.empty")}</div>}
        </main>
      </div>
      {creating ? <NewRequest ctx={ctx} onClose={() => setCreating(false)} /> : null}
      {formOpen ? <FormSettings ctx={ctx} onClose={() => setFormOpen(false)} /> : null}
    </div>
  );
}

function NewRequest({ ctx, onClose }: { ctx: PropsCtx; onClose: () => void }) {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const nav = useNavigate();
  const pid = ctx.project.id;
  const [title, setTitle] = useState("");
  const box = useRef<EditorHandle>(null);
  const upload = useEditorUpload(ws.slug, pid);
  const send = useMutation({
    mutationFn: () => post<IntakeEntry>(intakePath(ws.slug, pid), { title, description_html: box.current?.value()?.html ?? "" }),
    onSuccess: (e) => {
      qc.invalidateQueries({ queryKey: iq.all(ws.slug, pid) });
      onClose();
      nav(`/${ws.slug}/projects/${pid}/intake/${e.work_item_id}`);
    },
  });
  const submit = (ev: FormEvent) => {
    ev.preventDefault();
    send.mutate();
  };
  return (
    <Dialog open onClose={onClose} title={t("in.new")} width={640}>
      <form className="form" onSubmit={submit}>
        <ErrorAlert message={send.error ? errorText(send.error, t) : null} />
        <TextField label={t("in.titleLabel")} value={title} onChange={(e) => setTitle(e.target.value)} required maxLength={255} autoFocus />
        <div className="field">
          <label>{t("in.description")}</label>
          <div className="card">
            <Editor handle={box} ariaLabel={t("in.description")} onUpload={projAtLeast(ctx.project.my_role, "contributor") ? upload : undefined} />
          </div>
        </div>
        <div>
          <button className="btn btn-primary" disabled={send.isPending || !title.trim()}>{t("in.send")}</button>
        </div>
      </form>
    </Dialog>
  );
}

type Triage = "decline" | "duplicate" | "snooze" | null;

function IntakeDetail({ ctx, id, nameOf }: { ctx: PropsCtx; id: string; nameOf: (e: IntakeEntry) => string }) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const me = useMe().data!;
  const qc = useQueryClient();
  const nav = useNavigate();
  const pid = ctx.project.id;
  const q = useIntakeEntry(ws.slug, pid, id);
  const reader = projAtLeast(ctx.project.my_role, "commenter");
  // Commenters and up discuss the request on the work item itself.
  const item = useQuery({ queryKey: ["work-item", ws.slug, pid, id], queryFn: () => get<WorkItem>(itemsPath(ws.slug, pid, id)), enabled: reader && !!q.data });
  const [dialog, setDialog] = useState<Triage>(null);
  const [editing, setEditing] = useState(false);
  const [withdrawing, setWithdrawing] = useState(false);
  const refresh = () => qc.invalidateQueries({ queryKey: iq.all(ws.slug, pid) });
  const act = useMutation({
    mutationFn: ({ action, body }: { action: string; body?: object }) => post<IntakeEntry>(`${intakePath(ws.slug, pid, id)}/${action}`, body ?? {}),
    onSuccess: (e) => {
      qc.setQueryData(iq.one(ws.slug, pid, id), (old: IntakeEntry | undefined) => ({ ...old, ...e }));
      refresh();
      if (e.status === "accepted") qc.invalidateQueries({ queryKey: ["work-items", ws.slug, pid] });
      setDialog(null);
    },
  });
  const withdraw = useMutation({
    mutationFn: () => del(intakePath(ws.slug, pid, id)),
    onSuccess: () => {
      refresh();
      nav(`/${ws.slug}/projects/${pid}/intake`);
    },
  });
  if (q.isLoading) return <FullPageSpinner />;
  if (!q.data) return <div className="card empty">{q.error instanceof ApiError && q.error.status !== 404 ? errorText(q.error, t) : t("in.notFound")}</div>;
  const e = q.data;
  const isAdmin = ctx.project.my_role === "admin";
  const canEdit = mayEditIntake(e, me.id, isAdmin, !!ctx.project.archived_at);
  const key = `${ctx.project.identifier}-${e.sequence}`;
  const err = act.error ?? withdraw.error;
  return (
    <article className="intake-detail card">
      <div className="row-flex">
        <StatusBadge status={e.status} />
        <div className="page-actions">
          {isAdmin && !ctx.project.archived_at && e.status !== "accepted" ? (
            <>
              <button className="btn btn-sm btn-primary" onClick={() => act.mutate({ action: "accept" })} disabled={act.isPending}>{t("in.accept")}</button>
              {e.status !== "declined" ? <button className="btn btn-sm" onClick={() => setDialog("decline")}>{t("in.decline")}</button> : null}
              {e.status !== "duplicate" ? <button className="btn btn-sm" onClick={() => setDialog("duplicate")}>{t("in.duplicate")}</button> : null}
              {e.status === "pending" ? <button className="btn btn-sm" onClick={() => setDialog("snooze")}>{t("in.snooze")}</button> : null}
              {e.status !== "pending" ? <button className="btn btn-sm" onClick={() => act.mutate({ action: "reopen" })}>{t("in.reopen")}</button> : null}
            </>
          ) : null}
          {canEdit ? <button className="btn btn-sm" onClick={() => setEditing(true)}>{t("in.edit")}</button> : null}
          {canEdit ? <button className="btn btn-sm btn-ghost danger-text" onClick={() => setWithdrawing(true)}>{t("in.withdraw")}</button> : null}
        </div>
      </div>
      <ErrorAlert message={err ? errorText(err, t) : null} />
      <h2 className="intake-title">{e.title}</h2>
      <div className="faint small row-wrap">
        <Avatar name={nameOf(e)} size={18} />
        <span>{t("in.from", { name: nameOf(e) })}</span>
        {e.source === "form" ? <span>{t("in.viaForm")}</span> : null}
        {e.submitter_email ? <a href={`mailto:${e.submitter_email}`}>{e.submitter_email}</a> : null}
        <span title={new Date(e.created_at).toLocaleString()}>· {timeAgo(e.created_at, t, lang)}</span>
      </div>
      {e.status === "accepted" ? (
        <div className="alert alert-success">
          {reader ? <Link to={`/${ws.slug}/projects/${pid}/work-items/${id}`}>{t("in.acceptedAs", { key })}</Link> : t("in.acceptedAs", { key })}
        </div>
      ) : null}
      {e.status === "declined" && e.reason ? <div className="alert alert-warning">{t("in.declinedWhy", { reason: e.reason })}</div> : null}
      {e.status === "duplicate" && e.duplicate_sequence ? <div className="alert">{t("in.dupSeq", { key: `${ctx.project.identifier}-${e.duplicate_sequence}` })}</div> : null}
      {e.status === "snoozed" && e.snoozed_until ? <div className="alert">{t("in.snoozedUntil", { date: new Date(e.snoozed_until).toLocaleDateString(lang === "id" ? "id-ID" : "en-US") })}</div> : null}
      {/* Stored HTML is sanitized on the server. */}
      {e.description_html ? <div className="prose" dangerouslySetInnerHTML={{ __html: e.description_html }} /> : null}
      {reader && item.data ? <Comments slug={ws.slug} ctx={ctx} item={item.data} /> : null}

      {dialog ? <TriageDialog kind={dialog} busy={act.isPending} error={act.error} onClose={() => setDialog(null)} onSubmit={(body) => act.mutate({ action: dialog, body })} /> : null}
      {editing ? <EditRequest ctx={ctx} entry={e} onClose={() => setEditing(false)} /> : null}
      <ConfirmDialog
        open={withdrawing}
        onClose={() => setWithdrawing(false)}
        onConfirm={() => withdraw.mutate()}
        title={t("in.withdraw")}
        body={t("in.withdrawBody")}
        confirmLabel={t("in.withdraw")}
        danger
        busy={withdraw.isPending}
      />
    </article>
  );
}

function TriageDialog({ kind, busy, error, onClose, onSubmit }: { kind: Exclude<Triage, null>; busy: boolean; error: unknown; onClose: () => void; onSubmit: (body: object) => void }) {
  const { t } = useI18n();
  const tomorrow = addDays(todayIso(), 1);
  const [value, setValue] = useState(kind === "snooze" ? tomorrow : "");
  const title = kind === "decline" ? t("in.decline") : kind === "duplicate" ? t("in.duplicate") : t("in.snooze");
  const submit = (ev: FormEvent) => {
    ev.preventDefault();
    onSubmit(kind === "decline" ? { reason: value } : kind === "duplicate" ? { duplicate_of: value } : { until: value });
  };
  return (
    <Dialog open onClose={onClose} title={title}>
      <form className="form" onSubmit={submit}>
        <ErrorAlert message={error ? errorText(error, t) : null} />
        {kind === "decline" ? <TextField label={t("in.reason")} value={value} onChange={(e) => setValue(e.target.value)} maxLength={1000} autoFocus /> : null}
        {kind === "duplicate" ? <TextField label={t("in.dupOf")} value={value} onChange={(e) => setValue(e.target.value)} required autoFocus placeholder="WEB-12" /> : null}
        {kind === "snooze" ? (
          <div className="field">
            <label htmlFor="in-until">{t("in.until")}</label>
            <DateField id="in-until" value={value || null} min={tomorrow} required onChange={(d) => setValue(d ?? "")} />
          </div>
        ) : null}
        <div>
          <button className="btn btn-primary" disabled={busy}>{title}</button>
        </div>
      </form>
    </Dialog>
  );
}

function EditRequest({ ctx, entry, onClose }: { ctx: PropsCtx; entry: IntakeEntry; onClose: () => void }) {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const pid = ctx.project.id;
  const [title, setTitle] = useState(entry.title);
  const box = useRef<EditorHandle>(null);
  const save = useMutation({
    mutationFn: () => patch<IntakeEntry>(intakePath(ws.slug, pid, entry.work_item_id), { title, description_html: box.current?.value()?.html ?? "" }),
    onSuccess: (e) => {
      qc.setQueryData(iq.one(ws.slug, pid, entry.work_item_id), e);
      qc.invalidateQueries({ queryKey: iq.all(ws.slug, pid) });
      onClose();
    },
  });
  return (
    <Dialog open onClose={onClose} title={t("in.edit")} width={640}>
      <form
        className="form"
        onSubmit={(ev) => {
          ev.preventDefault();
          save.mutate();
        }}
      >
        <ErrorAlert message={save.error ? errorText(save.error, t) : null} />
        <TextField label={t("in.titleLabel")} value={title} onChange={(e) => setTitle(e.target.value)} required maxLength={255} autoFocus />
        <div className="field">
          <label>{t("in.description")}</label>
          <div className="card">
            <Editor handle={box} initialHtml={entry.description_html ?? ""} ariaLabel={t("in.description")} />
          </div>
        </div>
        <div>
          <button className="btn btn-primary" disabled={save.isPending || !title.trim()}>{t("in.save")}</button>
        </div>
      </form>
    </Dialog>
  );
}

function FormSettings({ ctx, onClose }: { ctx: PropsCtx; onClose: () => void }) {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const pid = ctx.project.id;
  const base = `${projectPath(ws.slug, pid)}/intake-form`;
  const q = useQuery({ queryKey: iq.form(ws.slug, pid), queryFn: () => get<{ enabled: boolean; path: string | null }>(base) });
  const set = useMutation({
    mutationFn: (enabled: boolean) => post<{ enabled: boolean; path: string | null }>(base, { enabled }),
    onSuccess: (v) => qc.setQueryData(iq.form(ws.slug, pid), v),
  });
  const [copied, setCopied] = useState(false);
  const url = q.data?.path ? `${location.origin}${q.data.path}` : "";
  return (
    <Dialog open onClose={onClose} title={t("in.formSettings")}>
      <div className="form">
        <ErrorAlert message={set.error ? errorText(set.error, t) : null} />
        {q.data?.enabled ? (
          <>
            <p className="small">{t("in.formOn")}</p>
            <div className="row-flex copy-row">
              <input className="input mono small" readOnly value={url} aria-label={t("in.formSettings")} onFocus={(e) => e.target.select()} />
              <button
                className="btn btn-sm"
                onClick={async () => {
                  await navigator.clipboard?.writeText(url).catch(() => {});
                  setCopied(true);
                }}
              >
                {copied ? t("in.copied") : t("in.copy")}
              </button>
            </div>
            <div className="page-actions">
              <button className="btn btn-sm" onClick={() => set.mutate(true)}>{t("in.formRenew")}</button>
              <button className="btn btn-sm btn-ghost danger-text" onClick={() => set.mutate(false)}>{t("in.formDisable")}</button>
            </div>
          </>
        ) : q.isSuccess ? (
          <>
            <p className="small">{t("in.formOff")}</p>
            <div>
              <button className="btn btn-primary" onClick={() => set.mutate(true)}>{t("in.formEnable")}</button>
            </div>
          </>
        ) : null}
      </div>
    </Dialog>
  );
}
