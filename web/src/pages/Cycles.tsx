import { Archive, ChevronDown } from "lucide-react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link, Navigate, useNavigate, useParams } from "react-router";
import { del, patch, post } from "../api/client";
import type { Cycle, CycleProgress, CycleStatus } from "../api/types";
import { STATE_GROUPS } from "../api/types";
import { BurndownChart } from "../components/BurndownChart";
import { ConfirmDialog, Dialog } from "../components/Dialog";
import { StateIcon, fmtDate } from "../components/Props";
import { ErrorAlert, FullPageSpinner, TextField } from "../components/ui";
import type { PropsCtx } from "../components/WorkItemProps";
import { useI18n } from "../i18n";
import { cq, cyclesPath, useCycle, useCycleProgress, useCycles } from "../lib/cycles";
import { errorText } from "../lib/errors";
import { pcan } from "../lib/perm";
import { useProjectCtx } from "../lib/projectCtx";
import { useProject } from "../lib/projects";
import { useMe } from "../lib/session";
import { toast } from "../lib/toast";
import { wiq } from "../lib/workItems";
import { ProjectBar, WorkItemsView } from "./ProjectHome";
import { useWs } from "./WorkspaceHome";
import { DateRangeField } from "../components/DatePicker";

const ORDER: CycleStatus[] = ["current", "upcoming", "draft", "completed"];

function useCtx(): PropsCtx | null | "missing" {
  const ws = useWs();
  const { projectId = "" } = useParams();
  const project = useProject(ws.slug, projectId);
  const ctx = useProjectCtx(ws.slug, project.data);
  if (project.isSuccess && (!project.data?.my_role || !project.data.feature_cycles)) return "missing";
  return ctx;
}

export function cycleDates(c: Cycle, lang: string, t: (k: "cyc.noDates") => string) {
  return c.start_date && c.end_date ? `${fmtDate(c.start_date, lang)} – ${fmtDate(c.end_date, lang)}` : t("cyc.noDates");
}

export function Meter({ done, total }: { done: number; total: number }) {
  const pct = total ? Math.round((done / total) * 100) : 0;
  return (
    <div className="meter" role="meter" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
      <span style={{ width: `${pct}%` }} />
    </div>
  );
}

export function CyclesPage() {
  const { t, lang } = useI18n();
  const ws = useWs();
  const ctx = useCtx();
  const [creating, setCreating] = useState(false);
  const p = ctx && ctx !== "missing" ? ctx.project : null;
  const list = useCycles(ws.slug, p?.id ?? "", !!p);
  if (ctx === "missing") return <Navigate to=".." relative="path" replace />;
  if (!ctx || !p) return <FullPageSpinner />;
  const cycles = list.data ?? [];
  return (
    <div className="proj-view">
      <ProjectBar ctx={ctx}>
        {pcan.viewArchives(p) ? (
          <Link to={`/${ws.slug}/projects/${p.id}/archives/cycles`} className="btn">
            <Archive />
            {t("ptab.archives")}
          </Link>
        ) : null}
        {pcan.createWorkItem(p) && !p.archived_at ? <button className="btn btn-primary" onClick={() => setCreating(true)}>{t("cyc.new")}</button> : null}
      </ProjectBar>
      <div className="page page-wide">
        {list.isSuccess && cycles.length === 0 ? <div className="card empty">{t("cyc.empty")}</div> : null}
        {ORDER.map((status) => {
          const inStatus = cycles.filter((c) => c.status === status);
          if (!inStatus.length) return null;
          return (
            <section key={status} aria-label={t(`cyc.status.${status}`)}>
              <h2 className="section-title">{t(`cyc.status.${status}`)}</h2>
              <div className="rows card">
                {inStatus.map((c) => (
                  <Link key={c.id} to={c.id} relative="path" className="row row-link cycle-row">
                    <div className="row-main">
                      <span className="cycle-name">{c.name}</span>
                      <span className="faint small">{cycleDates(c, lang, t)}</span>
                    </div>
                    <div className="cycle-meter">
                      <Meter done={c.completed_items} total={c.total_items} />
                      <span className="faint small">{t("cyc.items", { done: c.completed_items, total: c.total_items })}</span>
                    </div>
                  </Link>
                ))}
              </div>
            </section>
          );
        })}
      </div>
      <CycleDialog ctx={ctx} open={creating} onClose={() => setCreating(false)} />
    </div>
  );
}

function CycleDialog({ ctx, open, onClose, cycle }: { ctx: PropsCtx; open: boolean; onClose: () => void; cycle?: Cycle }) {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [f, setF] = useState({ name: cycle?.name ?? "", description: cycle?.description ?? "", start_date: cycle?.start_date ?? "", end_date: cycle?.end_date ?? "" });
  // Other dated cycles: the range picker warns about overlaps and offers "like the last one".
  const all = useCycles(ws.slug, ctx.project.id, open);
  const others = (all.data ?? []).filter((c) => c.id !== cycle?.id && c.start_date && c.end_date).map((c) => ({ name: c.name, start: c.start_date!, end: c.end_date! }));
  const last = others.reduce<(typeof others)[number] | null>((a, c) => (!a || c.end > a.end ? c : a), null);
  const save = useMutation({
    mutationFn: () => {
      const body = { name: f.name, description: f.description, start_date: f.start_date || null, end_date: f.end_date || null };
      return cycle ? patch<Cycle>(cyclesPath(ws.slug, ctx.project.id, cycle.id), body) : post<Cycle>(cyclesPath(ws.slug, ctx.project.id), body);
    },
    onSuccess: (c) => {
      qc.invalidateQueries({ queryKey: cq.all(ws.slug, ctx.project.id) });
      qc.setQueryData(cq.one(ws.slug, ctx.project.id, c.id), c);
      onClose();
      if (!cycle) navigate(`/${ws.slug}/projects/${ctx.project.id}/cycles/${c.id}`);
    },
  });
  return (
    <Dialog open={open} onClose={onClose} title={cycle ? cycle.name : t("cyc.new")}>
      <form className="form" onSubmit={(e: FormEvent) => { e.preventDefault(); save.mutate(); }}>
        <ErrorAlert message={save.isError ? errorText(save.error, t) : null} />
        <TextField label={t("cyc.name")} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} required maxLength={80} autoFocus />
        <div className="form-row">
          <div className="field grow">
            <label id="cyc-dates">{t("date.dates")}</label>
            <DateRangeField
              aria-labelledby="cyc-dates"
              startLabel={t("cyc.start")}
              endLabel={t("cyc.end")}
              value={{ start: f.start_date || null, end: f.end_date || null }}
              onChange={(r) => setF({ ...f, start_date: r.start ?? "", end_date: r.end ?? "" })}
              conflicts={others}
              last={last}
            />
          </div>
        </div>
        <div className="field">
          <label htmlFor="cyc-desc">{t("cyc.description")} ({t("common.optional")})</label>
          <textarea id="cyc-desc" className="textarea" value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} maxLength={4000} />
        </div>
        <div><button className="btn btn-primary" disabled={save.isPending || !f.name.trim()}>{cycle ? t("common.save") : t("cyc.new")}</button></div>
      </form>
    </Dialog>
  );
}

export function CyclePage() {
  const ws = useWs();
  const ctx = useCtx();
  const { cycleId = "" } = useParams();
  const p = ctx && ctx !== "missing" ? ctx.project : null;
  const cycle = useCycle(ws.slug, p?.id ?? "x", cycleId);
  if (ctx === "missing") return <Navigate to="../.." relative="path" replace />;
  if (!ctx || cycle.isLoading) return <FullPageSpinner />;
  if (!cycle.data) return <Navigate to=".." relative="path" replace />;
  const c = cycle.data;
  return (
    <WorkItemsView
      key={c.id}
      ctx={ctx}
      scope={{ key: `cycle:${c.id}`, query: `cycle=${c.id}`, defaults: { cycle_id: c.id }, header: <CycleHeader ctx={ctx} c={c} />, crumbs: [{ label: c.name }] }}
    />
  );
}

function CycleHeader({ ctx, c }: { ctx: PropsCtx; c: Cycle }) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const me = useMe().data!;
  const qc = useQueryClient();
  const navigate = useNavigate();
  const pid = ctx.project.id;
  const progress = useCycleProgress(ws.slug, pid, c.id);
  const [open, setOpen] = useState(true);
  const [editing, setEditing] = useState(false);
  const [transfer, setTransfer] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const archived = !!c.archived_at;
  const editable = ctx.editable && !archived;
  const canDelete = pcan.deleteWorkItem(ctx.project, c.created_by === me.id) && !ctx.project.archived_at && !archived;
  const archiveBase = `/${ws.slug}/projects/${pid}/archives/cycles`;
  const setArchived = (on: boolean) =>
    act(async () => {
      await post(cyclesPath(ws.slug, pid, c.id) + (on ? "/archive" : "/unarchive"));
      if (on) {
        toast({ kind: "success", title: t("arch.cycleArchived"), link: { label: t("arch.view"), to: archiveBase } });
        navigate(`/${ws.slug}/projects/${pid}/cycles`);
      } else {
        toast({ kind: "success", title: t("arch.cycleRestored") });
      }
    });
  const refresh = () => {
    qc.invalidateQueries({ queryKey: cq.all(ws.slug, pid) });
    qc.invalidateQueries({ queryKey: ["cycle", ws.slug, pid] });
    qc.invalidateQueries({ queryKey: wiq.allLists(ws.slug, pid) });
  };
  const act = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      setError(null);
      refresh();
    } catch (e) {
      setError(errorText(e, t));
    }
  };
  const pr = progress.data;
  const targets = (ctx.cycles ?? []).filter((x) => x.id !== c.id && x.status !== "completed");
  return (
    <div className="cycle-head">
      <div className="cycle-title">
        <b>{c.name}</b>
        <span className="badge">{t(`cyc.status.${c.status}`)}</span>
        <span className="faint small">{cycleDates(c, lang, t)}</span>
        <span className="grow" />
        <button className="btn btn-sm btn-ghost" onClick={() => setOpen((o) => !o)} aria-expanded={open}>{t("cyc.progress")} <ChevronDown className={open ? "rot180" : ""} /></button>
        {editable ? <button className="btn btn-sm btn-ghost" onClick={() => setEditing(true)}>{t("common.edit")}</button> : null}
        {editable && pr && pr.total_items > pr.completed_items && targets.length ? (
          <button className="btn btn-sm btn-ghost" onClick={() => setTransfer(true)}>{t("cyc.transfer")}</button>
        ) : null}
        {editable ? (
          <button className="btn btn-sm btn-ghost" disabled={c.status !== "completed"} title={c.status !== "completed" ? t("arch.cycleOnlyCompleted") : undefined} onClick={() => setArchived(true)}>
            {t("cyc.archive")}
          </button>
        ) : null}
        {canDelete ? <button className="btn btn-sm btn-ghost" onClick={() => setConfirm(true)}>{t("common.delete")}</button> : null}
      </div>
      <ErrorAlert message={error} />
      {archived ? (
        <div className="wi-archived">
          <Archive aria-hidden />
          <span className="grow">{t("arch.readOnlyCycle")}</span>
          <Link className="btn btn-sm btn-ghost" to={archiveBase}>{t("arch.goTo")}</Link>
          {ctx.editable ? <button className="btn btn-sm" onClick={() => setArchived(false)}>{t("cyc.unarchive")}</button> : null}
        </div>
      ) : null}
      {note ? <div className="alert alert-success">{note}</div> : null}
      {open && pr ? <ProgressPanels pr={pr} points={ctx.project.estimate_type === "points"} /> : null}
      {editing ? <CycleDialog ctx={ctx} open={editing} onClose={() => setEditing(false)} cycle={c} /> : null}
      <Dialog open={transfer} onClose={() => setTransfer(false)} title={t("cyc.transferTo")}>
        <div className="rows card">
          {targets.map((x) => (
            <button key={x.id} className="row row-btn" onClick={async () => {
              setTransfer(false);
              await act(async () => {
                const r = await post<{ moved: number }>(cyclesPath(ws.slug, pid, c.id) + "/transfer", { to_cycle_id: x.id });
                setNote(t("cyc.transferDone", { n: r.moved }));
              });
            }}>
              <span className="grow">{x.name}</span>
              <span className="faint small">{t(`cyc.status.${x.status}`)}</span>
            </button>
          ))}
        </div>
      </Dialog>
      <ConfirmDialog open={confirm} onClose={() => setConfirm(false)} title={t("cyc.delete")} body={t("cyc.deleteBody")} confirmLabel={t("common.delete")} danger
        onConfirm={async () => { setConfirm(false); await act(() => del(cyclesPath(ws.slug, pid, c.id))); navigate(`/${ws.slug}/projects/${pid}/cycles`); }} />
    </div>
  );
}

/** Completion meter, counts per state group and the burn-down, for a cycle or a module. */
export function ProgressPanels({ pr, points }: { pr: CycleProgress; points: boolean }) {
  const { t } = useI18n();
  const pct = pr.total_items ? Math.round((pr.completed_items / pr.total_items) * 100) : 0;
  return (
    <div className="cycle-panels">
      <section className="card card-body cycle-progress" aria-label={t("cyc.progress")}>
        <div className="row-flex">
          <b>{t("cyc.complete", { pct })}</b>
          <span className="faint small">{t("cyc.items", { done: pr.completed_items, total: pr.total_items })}{points ? ` · ${t("cyc.points", { n: `${pr.completed_points}/${pr.total_points}` })}` : ""}</span>
        </div>
        <Meter done={pr.completed_items} total={pr.total_items} />
        <dl className="group-counts">
          {STATE_GROUPS.map((g) => {
            const gc = pr.groups.find((x) => x.group === g);
            return (
              <div key={g}>
                <dt><StateIcon group={g} color={`var(--group-${g})`} /> {t(`group.${g}`)}</dt>
                <dd>{gc?.items ?? 0}</dd>
              </div>
            );
          })}
        </dl>
      </section>
      <section className="card card-body" aria-label={t("cyc.burndown")}>
        <b>{t("cyc.burndown")}</b>
        {pr.burndown.length ? <BurndownChart points={pr.burndown} /> : <p className="faint small">{t("cyc.noBurndown")}</p>}
      </section>
    </div>
  );
}
