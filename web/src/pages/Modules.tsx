import { Archive, ChevronDown, X } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link, Navigate, useNavigate, useParams } from "react-router";
import { del, get, patch, post } from "../api/client";
import type { Module, ModuleStatus } from "../api/types";
import { MODULE_STATUSES } from "../api/types";
import { Avatar } from "../components/Avatar";
import { ConfirmDialog, Dialog } from "../components/Dialog";
import { Picker } from "../components/Picker";
import { fmtDate } from "../components/Props";
import { ErrorAlert, FullPageSpinner, TextField } from "../components/ui";
import type { PropsCtx } from "../components/WorkItemProps";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { mq, modulesPath, useModule, useModuleProgress, useModules } from "../lib/modules";
import { pcan } from "../lib/perm";
import { useProjectCtx } from "../lib/projectCtx";
import { useProject } from "../lib/projects";
import { useMe } from "../lib/session";
import { toast } from "../lib/toast";
import { wiq } from "../lib/workItems";
import { Meter, ProgressPanels } from "./Cycles";
import { ProjectBar, WorkItemsView } from "./ProjectHome";
import { useWs } from "./WorkspaceHome";
import { Select } from "../components/Select";
import { DateRangeField } from "../components/DatePicker";

function useCtx(): PropsCtx | null | "missing" {
  const ws = useWs();
  const { projectId = "" } = useParams();
  const project = useProject(ws.slug, projectId);
  const ctx = useProjectCtx(ws.slug, project.data);
  if (project.isSuccess && (!project.data?.my_role || !project.data.feature_modules)) return "missing";
  return ctx;
}

export function ModuleStatusBadge({ status }: { status: ModuleStatus }) {
  const { t } = useI18n();
  return <span className={`mod-status mod-${status}`}>{t(`mod.st.${status}`)}</span>;
}

export function ModulesPage() {
  const { t, lang } = useI18n();
  const ws = useWs();
  const ctx = useCtx();
  const [creating, setCreating] = useState(false);
  const p = ctx && ctx !== "missing" ? ctx.project : null;
  const list = useModules(ws.slug, p?.id ?? "", !!p);
  if (ctx === "missing") return <Navigate to=".." relative="path" replace />;
  if (!ctx || !p) return <FullPageSpinner />;
  const mods = list.data ?? [];
  return (
    <div className="proj-view">
      <ProjectBar ctx={ctx}>
        {pcan.viewArchives(p) ? (
          <Link to={`/${ws.slug}/projects/${p.id}/archives/modules`} className="btn">
            <Archive />
            {t("ptab.archives")}
          </Link>
        ) : null}
        {pcan.createWorkItem(p) && !p.archived_at ? <button className="btn btn-primary" onClick={() => setCreating(true)}>{t("mod.new")}</button> : null}
      </ProjectBar>
      <div className="page page-wide">
        {list.isSuccess && mods.length === 0 ? <div className="card empty">{t("mod.empty")}</div> : null}
        <div className="proj-grid">
          {mods.map((m) => {
            const lead = ctx.members.find((x) => x.user_id === m.lead_id);
            return (
              <Link key={m.id} to={m.id} relative="path" className="card proj-card mod-card">
                <div className="proj-card-body">
                  <div className="row-flex">
                    <span className="proj-name">{m.name}</span>
                    <ModuleStatusBadge status={m.status} />
                  </div>
                  <span className="faint small">
                    {m.start_date && m.target_date ? `${fmtDate(m.start_date, lang)} – ${fmtDate(m.target_date, lang)}` : t("cyc.noDates")}
                  </span>
                  <Meter done={m.completed_items} total={m.total_items} />
                  <div className="row-flex">
                    <span className="faint small">{t("cyc.items", { done: m.completed_items, total: m.total_items })}</span>
                    {lead ? <Avatar name={lead.name} email={lead.email} src={lead.avatar} size={20} /> : null}
                  </div>
                </div>
              </Link>
            );
          })}
        </div>
      </div>
      <ModuleDialog ctx={ctx} open={creating} onClose={() => setCreating(false)} />
    </div>
  );
}

function ModuleDialog({ ctx, open, onClose, module: m }: { ctx: PropsCtx; open: boolean; onClose: () => void; module?: Module }) {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const me = useMe().data!;
  const [f, setF] = useState({
    name: m?.name ?? "", description: m?.description ?? "", status: m?.status ?? ("backlog" as ModuleStatus),
    start_date: m?.start_date ?? "", target_date: m?.target_date ?? "", lead_id: m ? (m.lead_id ?? "") : me.id, member_ids: m?.member_ids ?? ([] as string[]),
  });
  const save = useMutation({
    mutationFn: () => {
      const body = { ...f, start_date: f.start_date || null, target_date: f.target_date || null, lead_id: f.lead_id || null };
      return m ? patch<Module>(modulesPath(ws.slug, ctx.project.id, m.id), body) : post<Module>(modulesPath(ws.slug, ctx.project.id), body);
    },
    onSuccess: (x) => {
      qc.invalidateQueries({ queryKey: mq.all(ws.slug, ctx.project.id) });
      qc.setQueryData(mq.one(ws.slug, ctx.project.id, x.id), x);
      onClose();
      if (!m) navigate(`/${ws.slug}/projects/${ctx.project.id}/modules/${x.id}`);
    },
  });
  const people = ctx.members.map((x) => ({ value: x.user_id, label: x.name || x.email, icon: <Avatar name={x.name} email={x.email} src={x.avatar} size={18} /> }));
  return (
    <Dialog open={open} onClose={onClose} title={m ? m.name : t("mod.new")} width={520}>
      <form className="form" onSubmit={(e: FormEvent) => { e.preventDefault(); save.mutate(); }}>
        <ErrorAlert message={save.isError ? errorText(save.error, t) : null} />
        <TextField label={t("mod.name")} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} required maxLength={80} autoFocus />
        <div className="form-row">
          <div className="field grow">
            <label htmlFor="mod-status">{t("mod.status")}</label>
            <Select id="mod-status" value={f.status} onChange={(v) => setF({ ...f, status: v as ModuleStatus })}>
              {MODULE_STATUSES.map((s) => <option key={s} value={s}>{t(`mod.st.${s}`)}</option>)}
            </Select>
          </div>
          <div className="field grow">
            <label htmlFor="mod-lead">{t("mod.lead")}</label>
            <Select id="mod-lead" value={f.lead_id} onChange={(v) => setF({ ...f, lead_id: v })}>
              <option value="">{t("proj.noLead")}</option>
              {ctx.members.map((x) => <option key={x.user_id} value={x.user_id}>{x.name || x.email}</option>)}
            </Select>
          </div>
        </div>
        <div className="form-row">
          <div className="field grow">
            <label id="mod-dates">{t("date.dates")}</label>
            <DateRangeField
              aria-labelledby="mod-dates"
              startLabel={t("mod.start")}
              endLabel={t("mod.target")}
              value={{ start: f.start_date || null, end: f.target_date || null }}
              onChange={(r) => setF({ ...f, start_date: r.start ?? "", target_date: r.end ?? "" })}
            />
          </div>
        </div>
        <div className="field">
          <label>{t("mod.members")}</label>
          <Picker label={t("mod.members")} className="btn btn-sm" multi options={people} value={f.member_ids} onChange={(v) => setF({ ...f, member_ids: v })}
            trigger={<span>{f.member_ids.length ? f.member_ids.map((id) => ctx.members.find((x) => x.user_id === id)?.name || "—").join(", ") : "—"}</span>} />
        </div>
        <div className="field">
          <label htmlFor="mod-desc">{t("cyc.description")} ({t("common.optional")})</label>
          <textarea id="mod-desc" className="textarea" value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} maxLength={4000} />
        </div>
        <div><button className="btn btn-primary" disabled={save.isPending || !f.name.trim()}>{m ? t("common.save") : t("mod.new")}</button></div>
      </form>
    </Dialog>
  );
}

export function ModulePage() {
  const ws = useWs();
  const ctx = useCtx();
  const { moduleId = "" } = useParams();
  const p = ctx && ctx !== "missing" ? ctx.project : null;
  const mod = useModule(ws.slug, p?.id ?? "x", moduleId);
  if (ctx === "missing") return <Navigate to="../.." relative="path" replace />;
  if (!ctx || mod.isLoading) return <FullPageSpinner />;
  if (!mod.data) return <Navigate to=".." relative="path" replace />;
  const m = mod.data;
  return <WorkItemsView key={m.id} ctx={ctx} scope={{ key: `module:${m.id}`, query: `module=${m.id}`, defaults: { module_ids: [m.id] }, header: <ModuleHeader ctx={ctx} m={m} />, crumbs: [{ label: m.name }] }} />;
}

function ModuleHeader({ ctx, m }: { ctx: PropsCtx; m: Module }) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const me = useMe().data!;
  const qc = useQueryClient();
  const navigate = useNavigate();
  const pid = ctx.project.id;
  const progress = useModuleProgress(ws.slug, pid, m.id);
  const links = useQuery({ queryKey: mq.links(ws.slug, pid, m.id), queryFn: () => get<{ id: string; url: string; title: string }[]>(modulesPath(ws.slug, pid, m.id) + "/links") });
  const [open, setOpen] = useState(true);
  const [editing, setEditing] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [url, setUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const lead = ctx.members.find((x) => x.user_id === m.lead_id);
  const archived = !!m.archived_at;
  const editable = ctx.editable && !archived;
  const closed = m.status === "completed" || m.status === "cancelled";
  const archiveBase = `/${ws.slug}/projects/${pid}/archives/modules`;
  const act = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      setError(null);
      qc.invalidateQueries({ queryKey: mq.all(ws.slug, pid) });
      qc.invalidateQueries({ queryKey: ["module", ws.slug, pid] });
      qc.invalidateQueries({ queryKey: wiq.allLists(ws.slug, pid) });
    } catch (e) {
      setError(errorText(e, t));
    }
  };
  return (
    <div className="cycle-head">
      <div className="cycle-title">
        <b>{m.name}</b>
        <ModuleStatusBadge status={m.status} />
        <span className="faint small">{m.start_date && m.target_date ? `${fmtDate(m.start_date, lang)} – ${fmtDate(m.target_date, lang)}` : ""}</span>
        {lead ? <span className="prop-inline small"><Avatar name={lead.name} email={lead.email} src={lead.avatar} size={18} />{lead.name || lead.email}</span> : null}
        <span className="grow" />
        <button className="btn btn-sm btn-ghost" onClick={() => setOpen((o) => !o)} aria-expanded={open}>{t("cyc.progress")} <ChevronDown className={open ? "rot180" : ""} /></button>
        {editable ? <button className="btn btn-sm btn-ghost" onClick={() => setEditing(true)}>{t("common.edit")}</button> : null}
        {editable ? (
          <button
            className="btn btn-sm btn-ghost"
            disabled={!closed}
            title={closed ? undefined : t("arch.moduleOnlyClosed")}
            onClick={() =>
              act(async () => {
                await post(modulesPath(ws.slug, pid, m.id) + "/archive");
                toast({ kind: "success", title: t("arch.moduleArchived"), link: { label: t("arch.view"), to: archiveBase } });
                navigate(`/${ws.slug}/projects/${pid}/modules`);
              })
            }
          >
            {t("cyc.archive")}
          </button>
        ) : null}
        {pcan.deleteWorkItem(ctx.project, m.created_by === me.id) && !ctx.project.archived_at && !archived ? <button className="btn btn-sm btn-ghost" onClick={() => setConfirm(true)}>{t("common.delete")}</button> : null}
      </div>
      <ErrorAlert message={error} />
      {archived ? (
        <div className="wi-archived">
          <Archive aria-hidden />
          <span className="grow">{t("arch.readOnlyModule")}</span>
          <Link className="btn btn-sm btn-ghost" to={archiveBase}>{t("arch.goTo")}</Link>
          {ctx.editable ? (
            <button className="btn btn-sm" onClick={() => act(async () => { await post(modulesPath(ws.slug, pid, m.id) + "/unarchive"); toast({ kind: "success", title: t("arch.moduleRestored") }); })}>
              {t("cyc.unarchive")}
            </button>
          ) : null}
        </div>
      ) : null}
      {open && progress.data ? <ProgressPanels pr={progress.data} points={ctx.project.estimate_type === "points"} /> : null}
      {open ? (
        <div className="mod-links small">
          <b>{t("mod.links")}</b>
          {(links.data ?? []).map((l) => (
            <span key={l.id} className="label-pill">
              <a href={l.url} target="_blank" rel="noopener noreferrer nofollow">{l.title || l.url}</a>
              {editable ? <button className="link-btn" aria-label={t("common.remove")} onClick={() => act(() => del(`${modulesPath(ws.slug, pid, m.id)}/links/${l.id}`))}><X /></button> : null}
            </span>
          ))}
          {editable ? (
            <form className="link-form" onSubmit={(e) => { e.preventDefault(); act(async () => { await post(`${modulesPath(ws.slug, pid, m.id)}/links`, { url }); setUrl(""); qc.invalidateQueries({ queryKey: mq.links(ws.slug, pid, m.id) }); }); }}>
              <input className="input" style={{ height: 26, width: 260 }} placeholder="https://" aria-label={t("links.url")} value={url} onChange={(e) => setUrl(e.target.value)} />
              <button className="btn btn-sm" disabled={!url.trim()}>{t("common.add")}</button>
            </form>
          ) : null}
        </div>
      ) : null}
      {editing ? <ModuleDialog ctx={ctx} open={editing} onClose={() => setEditing(false)} module={m} /> : null}
      <ConfirmDialog open={confirm} onClose={() => setConfirm(false)} title={t("mod.delete")} body={t("mod.deleteBody")} confirmLabel={t("common.delete")} danger
        onConfirm={async () => { setConfirm(false); await act(() => del(modulesPath(ws.slug, pid, m.id))); navigate(`/${ws.slug}/projects/${pid}/modules`); }} />
    </div>
  );
}
