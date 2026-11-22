import { X } from "lucide-react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ProjectBar } from "../components/PageHeader";
import { ProjectSettingsNav, SettingsLayout } from "../components/SettingsFrame";
import { DataTab, PublishTab } from "./ProjectData";
import { Automations } from "./ProjectAutomations";
import { useState, type FormEvent } from "react";
import { Navigate, useNavigate, useParams } from "react-router";
import { del, patch, post } from "../api/client";
import type { EstimateType, Label, Project, ProjRole, StateGroup, WfState, WsMember } from "../api/types";
import { STATE_GROUPS } from "../api/types";
import { Avatar } from "../components/Avatar";
import { ColorPicker, Dot } from "../components/ColorPicker";
import { CoverPicker } from "../components/CoverPicker";
import { ConfirmDialog, Dialog } from "../components/Dialog";
import { ErrorAlert, FullPageSpinner, TextField } from "../components/ui";
import { useT } from "../i18n";
import { errorText } from "../lib/errors";
import { pcan, projRolesFor } from "../lib/perm";
import { PALETTE, pq, projectPath, useLabels, useProject, useProjectMembers, useStates } from "../lib/projects";
import { wq } from "../lib/queries";
import { useMe } from "../lib/session";
import { useQuery } from "@tanstack/react-query";
import { get } from "../api/client";
import { useWs } from "./WorkspaceHome";
import { Select } from "../components/Select";

type Tab = "general" | "members" | "states" | "labels" | "estimates" | "features" | "automations" | "data" | "publish";
const TABS: Tab[] = ["general", "members", "states", "labels", "estimates", "features", "automations", "data", "publish"];

export function ProjectSettingsPage() {
  const t = useT();
  const ws = useWs();
  const { projectId = "", tab = "general" } = useParams();
  const project = useProject(ws.slug, projectId);
  if (project.isLoading) return <FullPageSpinner />;
  if (!project.data) return <Navigate to={`/${ws.slug}/projects`} replace />;
  const p = project.data;
  if (!p.my_role) return <Navigate to={`/${ws.slug}/projects/${p.id}`} replace />;
  return (
    <div className="proj-view">
      <ProjectBar ctx={{ project: p }} crumbs={[{ label: t(`proj.tab.${tab as Tab}`) }]} />
      <SettingsLayout nav={<ProjectSettingsNav tabs={TABS} />}>
      {tab === "general" ? <General p={p} /> : null}
      {tab === "members" ? <Members p={p} /> : null}
      {tab === "states" ? <States p={p} /> : null}
      {tab === "labels" ? <Labels p={p} /> : null}
      {tab === "estimates" ? <Estimates p={p} /> : null}
      {tab === "features" ? <Features p={p} /> : null}
      {tab === "automations" ? <Automations p={p} /> : null}
      {tab === "data" ? <DataTab p={p} /> : null}
      {tab === "publish" ? <PublishTab p={p} /> : null}
      </SettingsLayout>
    </div>
  );
}

function useProjectPatch(p: Project) {
  const ws = useWs();
  const qc = useQueryClient();
  const key = pq.one(ws.slug, p.id);
  return useMutation({
    mutationFn: (body: Record<string, unknown>) => patch<Project>(projectPath(ws.slug, p.id), body),
    // Optimistic: show the change at once, roll back if the server refuses.
    onMutate: async (body) => {
      await qc.cancelQueries({ queryKey: key });
      const prev = qc.getQueryData<Project>(key);
      if (prev) qc.setQueryData(key, { ...prev, ...body });
      return { prev };
    },
    onError: (_e, _b, ctx) => {
      if (ctx?.prev) qc.setQueryData(key, ctx.prev);
    },
    onSuccess: (np) => {
      qc.setQueryData(key, np);
      qc.invalidateQueries({ queryKey: pq.all(ws.slug) });
    },
  });
}

function General({ p }: { p: Project }) {
  const t = useT();
  const ws = useWs();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const editable = pcan.editSettings(p);
  const members = useProjectMembers(ws.slug, p.id);
  const [f, setF] = useState({
    name: p.name,
    identifier: p.identifier,
    description: p.description,
    icon: p.icon ?? "",
    cover_color: p.cover_color ?? PALETTE[0]!,
    lead_id: p.lead_id ?? "",
    default_assignee_id: p.default_assignee_id ?? "",
    is_private: p.is_private,
  });
  const save = useProjectPatch(p);
  const [confirm, setConfirm] = useState<"delete" | "leave" | null>(null);
  const after = async (to: string) => {
    await qc.invalidateQueries({ queryKey: pq.all(ws.slug) });
    navigate(to, { replace: true });
  };
  const archive = useMutation({
    mutationFn: () => post<Project>(projectPath(ws.slug, p.id) + (p.archived_at ? "/unarchive" : "/archive")),
    onSuccess: (np) => {
      qc.setQueryData(pq.one(ws.slug, p.id), np);
      qc.invalidateQueries({ queryKey: pq.all(ws.slug) });
    },
  });
  const remove = useMutation({ mutationFn: () => del<void>(projectPath(ws.slug, p.id)), onSuccess: () => after(`/${ws.slug}/projects`) });
  const leave = useMutation({ mutationFn: () => post<void>(projectPath(ws.slug, p.id) + "/leave"), onSuccess: () => after(`/${ws.slug}/projects`) });

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    save.mutate({ ...f, icon: f.icon || null, lead_id: f.lead_id || null, default_assignee_id: f.default_assignee_id || null });
  };
  return (
    <>
      <section className="card">
        <form className="card-body form" onSubmit={onSubmit}>
          <ErrorAlert message={save.isError ? errorText(save.error, t) : null} />
          {save.isSuccess ? <div className="alert alert-success">{t("common.saved")}</div> : null}
          <fieldset disabled={!editable} className="form" style={{ border: 0, padding: 0, margin: 0 }}>
            <div className="form-row">
              <div style={{ width: 72 }}>
                <TextField label={t("proj.icon")} value={f.icon} onChange={(e) => setF({ ...f, icon: e.target.value })} maxLength={8} />
              </div>
              <div className="grow">
                <TextField label={t("proj.name")} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} required maxLength={80} />
              </div>
              <div style={{ width: 120 }}>
                <TextField label={t("proj.identifier")} value={f.identifier} onChange={(e) => setF({ ...f, identifier: e.target.value.toUpperCase() })} required maxLength={12} />
              </div>
            </div>
            <div className="field">
              <label htmlFor="pd">{t("proj.description")}</label>
              <textarea id="pd" className="textarea" value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} maxLength={4000} />
            </div>
            <div className="field">
              <label>{t("proj.cover")}</label>
              <ColorPicker label={t("proj.cover")} value={f.cover_color} onChange={(c) => setF({ ...f, cover_color: c })} />
            </div>
            <div className="field">
              <label>{t("cover.image")}</label>
              <CoverPicker slug={ws.slug} p={p} disabled={!editable} />
            </div>
            <div className="form-row">
              <div className="field grow">
                <label htmlFor="plead">{t("proj.lead")}</label>
                <Select id="plead" value={f.lead_id} onChange={(v) => setF({ ...f, lead_id: v })}>
                  <option value="">{t("proj.noLead")}</option>
                  {(members.data ?? []).map((m) => (
                    <option key={m.user_id} value={m.user_id}>{m.name || m.email}</option>
                  ))}
                </Select>
              </div>
              <div className="field grow">
                <label htmlFor="pdef">{t("proj.defaultAssignee")}</label>
                <Select id="pdef" value={f.default_assignee_id} onChange={(v) => setF({ ...f, default_assignee_id: v })}>
                  <option value="">{t("proj.none")}</option>
                  {(members.data ?? []).map((m) => (
                    <option key={m.user_id} value={m.user_id}>{m.name || m.email}</option>
                  ))}
                </Select>
              </div>
            </div>
            <label className="checkbox">
              <input type="checkbox" checked={f.is_private} onChange={(e) => setF({ ...f, is_private: e.target.checked })} />
              <span>
                {t("proj.private")}
                <br />
                <span className="faint small">{f.is_private ? t("proj.privateHint") : t("proj.publicHint")}</span>
              </span>
            </label>
          </fieldset>
          {editable ? (
            <div>
              <button className="btn btn-primary" disabled={save.isPending}>{t("common.save")}</button>
            </div>
          ) : null}
        </form>
      </section>

      <section className="card card-danger">
        <div className="card-head">{t("ws.danger")}</div>
        <ErrorAlert message={[archive, remove, leave].find((m) => m.isError) ? errorText([archive, remove, leave].find((m) => m.isError)!.error, t) : null} />
        <div className="rows">
          {p.is_member ? (
            <div className="row">
              <div className="row-main"><b>{t("proj.leave")}</b></div>
              <button className="btn" onClick={() => setConfirm("leave")}>{t("proj.leave")}</button>
            </div>
          ) : null}
          {editable ? (
            <>
              <div className="row">
                <div className="row-main">
                  <b>{p.archived_at ? t("proj.unarchive") : t("proj.archive")}</b>
                  <span className="muted small">{t("proj.archiveBody")}</span>
                </div>
                <button className="btn" onClick={() => archive.mutate()} disabled={archive.isPending}>
                  {p.archived_at ? t("proj.unarchive") : t("proj.archive")}
                </button>
              </div>
              <div className="row">
                <div className="row-main">
                  <b>{t("proj.delete")}</b>
                  <span className="muted small">{t("proj.deleteBody")}</span>
                </div>
                <button className="btn btn-danger-outline" onClick={() => setConfirm("delete")}>{t("proj.delete")}</button>
              </div>
            </>
          ) : null}
        </div>
      </section>
      <ConfirmDialog
        open={confirm === "delete"}
        onClose={() => setConfirm(null)}
        onConfirm={() => {
          setConfirm(null);
          remove.mutate();
        }}
        title={t("proj.delete")}
        body={t("proj.deleteBody")}
        confirmLabel={t("common.delete")}
        danger
        typeToConfirm={{ label: t("proj.deleteConfirm", { identifier: p.identifier }), value: p.identifier }}
      />
      <ConfirmDialog
        open={confirm === "leave"}
        onClose={() => setConfirm(null)}
        onConfirm={() => {
          setConfirm(null);
          leave.mutate();
        }}
        title={t("proj.leave")}
        body={p.name}
        confirmLabel={t("proj.leave")}
        danger
      />
    </>
  );
}

function Members({ p }: { p: Project }) {
  const t = useT();
  const ws = useWs();
  const me = useMe().data!;
  const qc = useQueryClient();
  const members = useProjectMembers(ws.slug, p.id);
  const wsMembers = useQuery({ queryKey: wq.members(ws.slug), queryFn: () => get<WsMember[]>(`/workspaces/${ws.slug}/members`), enabled: pcan.manageMembers(p) });
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refresh = () => {
    qc.invalidateQueries({ queryKey: pq.members(ws.slug, p.id) });
    qc.invalidateQueries({ queryKey: pq.all(ws.slug) });
  };
  const setRole = useMutation({
    mutationFn: (v: { user: string; role: ProjRole }) => patch<void>(`${projectPath(ws.slug, p.id)}/members/${v.user}`, { role: v.role }),
    onSuccess: refresh,
    onError: (e) => setError(errorText(e, t)),
  });
  const remove = useMutation({
    mutationFn: (user: string) => del<void>(`${projectPath(ws.slug, p.id)}/members/${user}`),
    onSuccess: refresh,
    onError: (e) => setError(errorText(e, t)),
  });
  const manage = pcan.manageMembers(p);
  const candidates = (wsMembers.data ?? []).filter((w) => !(members.data ?? []).some((m) => m.user_id === w.user_id));
  return (
    <>
      <div className="page-head">
        <span className="grow" />
        {manage ? <button className="btn btn-primary" onClick={() => setAdding(true)}>{t("proj.addMember")}</button> : null}
      </div>
      <ErrorAlert message={error} />
      <section className="card rows">
        {(members.data ?? []).map((m) => (
          <div className="row" key={m.user_id}>
            <Avatar name={m.name} email={m.email} src={m.avatar} size={28} />
            <div className="row-main">
              <span className="ellipsis">{m.name || m.email} {m.user_id === me.id ? <span className="faint">({t("common.you")})</span> : null}</span>
              <span className="faint small">{m.email} · {t(`role.${m.workspace_role}`)}</span>
            </div>
            {manage ? (
              <Select className="select-inline" aria-label={t("members.role")} value={m.role} onChange={(v) => { setError(null); setRole.mutate({ user: m.user_id, role: v as ProjRole }); }}>
                {projRolesFor(m.workspace_role).map((r) => (
                  <option key={r} value={r}>{t(`prole.${r}`)}</option>
                ))}
              </Select>
            ) : (
              <span className="muted small">{t(`prole.${m.role}`)}</span>
            )}
            {manage && m.user_id !== me.id ? (
              <button className="btn btn-sm btn-ghost" onClick={() => { setError(null); remove.mutate(m.user_id); }}>{t("common.remove")}</button>
            ) : (
              <span style={{ width: 70 }} />
            )}
          </div>
        ))}
      </section>
      <AddMemberDialog open={adding} onClose={() => setAdding(false)} p={p} candidates={candidates} onDone={refresh} />
    </>
  );
}

function AddMemberDialog({ open, onClose, p, candidates, onDone }: { open: boolean; onClose: () => void; p: Project; candidates: WsMember[]; onDone: () => void }) {
  const t = useT();
  const ws = useWs();
  const [user, setUser] = useState("");
  const [role, setRole] = useState<ProjRole>("contributor");
  const chosen = candidates.find((c) => c.user_id === user);
  const roles = chosen ? projRolesFor(chosen.role) : (["admin", "contributor", "commenter", "guest"] as ProjRole[]);
  const add = useMutation({
    mutationFn: () => post<void>(`${projectPath(ws.slug, p.id)}/members`, { user_id: user, role: roles.includes(role) ? role : roles[0] }),
    onSuccess: () => {
      onDone();
      setUser("");
      onClose();
    },
  });
  return (
    <Dialog open={open} onClose={onClose} title={t("proj.addMember")}>
      {candidates.length === 0 ? (
        <p className="muted">{t("proj.everyoneIn")}</p>
      ) : (
        <form className="form" onSubmit={(e) => { e.preventDefault(); add.mutate(); }}>
          <ErrorAlert message={add.isError ? errorText(add.error, t) : null} />
          <div className="field">
            <label htmlFor="am-user">{t("proj.person")}</label>
            <Select id="am-user" value={user} onChange={(v) => setUser(v)} required>
              <option value="" disabled>—</option>
              {candidates.map((c) => (
                <option key={c.user_id} value={c.user_id}>{(c.name || c.email) + " · " + t(`role.${c.role}`)}</option>
              ))}
            </Select>
          </div>
          <div className="field">
            <label htmlFor="am-role">{t("members.role")}</label>
            <Select id="am-role" value={roles.includes(role) ? role : (roles[0] ?? "")} onChange={(v) => setRole(v as ProjRole)}>
              {roles.map((r) => (
                <option key={r} value={r}>{t(`prole.${r}`)}</option>
              ))}
            </Select>
          </div>
          <div>
            <button className="btn btn-primary" disabled={!user || add.isPending}>{t("common.add")}</button>
          </div>
        </form>
      )}
    </Dialog>
  );
}

function States({ p }: { p: Project }) {
  const t = useT();
  const ws = useWs();
  const qc = useQueryClient();
  const states = useStates(ws.slug, p.id);
  const editable = pcan.editSettings(p);
  const [error, setError] = useState<string | null>(null);
  const [adding, setAdding] = useState<StateGroup | null>(null);
  const url = `${projectPath(ws.slug, p.id)}/states`;
  const refresh = () => qc.invalidateQueries({ queryKey: pq.states(ws.slug, p.id) });
  const upd = useMutation({
    mutationFn: (v: { id: string; body: Record<string, unknown> }) => patch<WfState[]>(`${url}/${v.id}`, v.body),
    onSuccess: (all) => qc.setQueryData(pq.states(ws.slug, p.id), all),
    onError: (e) => setError(errorText(e, t)),
  });
  const remove = useMutation({ mutationFn: (id: string) => del<void>(`${url}/${id}`), onSuccess: refresh, onError: (e) => setError(errorText(e, t)) });
  const all = states.data ?? [];
  return (
    <>
      <ErrorAlert message={error} />
      <section className="card">
        {STATE_GROUPS.map((g) => {
          const inGroup = all.filter((s) => s.group === g);
          return (
            <div key={g}>
              <div className="group-head">
                <Dot color={`var(--group-${g})`} />
                <span className="grow">{t(`group.${g}`)}</span>
                {editable ? <button className="btn btn-sm btn-ghost" onClick={() => setAdding(g)}>+ {t("state.add")}</button> : null}
              </div>
              <div className="rows">
                {inGroup.map((s, i) => (
                  <div className="row" key={s.id}>
                    <input type="color" value={s.color.length === 7 ? s.color : "#888888"} disabled={!editable} aria-label={t("proj.cover")} className="color-input" onChange={(e) => upd.mutate({ id: s.id, body: { color: e.target.value } })} />
                    <input className="inline-input" defaultValue={s.name} disabled={!editable} aria-label={t("state.name")} onBlur={(e) => e.target.value.trim() && e.target.value !== s.name && upd.mutate({ id: s.id, body: { name: e.target.value } })} />
                    {s.is_default ? <span className="badge">{t("state.default")}</span> : editable ? <button className="btn btn-sm btn-ghost" onClick={() => upd.mutate({ id: s.id, body: { is_default: true } })}>{t("state.makeDefault")}</button> : null}
                    {editable ? (
                      <>
                        <button className="icon-btn" aria-label={t("common.moveUp")} disabled={i === 0} onClick={() => upd.mutate({ id: s.id, body: { sort_order: before(inGroup, i) } })}>↑</button>
                        <button className="icon-btn" aria-label={t("common.moveDown")} disabled={i === inGroup.length - 1} onClick={() => upd.mutate({ id: s.id, body: { sort_order: after(inGroup, i) } })}>↓</button>
                        <button className="icon-btn" aria-label={t("common.delete")} disabled={s.is_default} onClick={() => { setError(null); remove.mutate(s.id); }}><X /></button>
                      </>
                    ) : null}
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </section>
      <NameColorDialog
        open={!!adding}
        title={t("state.add")}
        label={t("state.name")}
        onClose={() => setAdding(null)}
        submit={(name, color) => post<WfState>(url, { name, color, group: adding })}
        onDone={refresh}
      />
    </>
  );
}

/** Sort keys between neighbours, for move up/down. */
function before(list: { sort_order: number }[], i: number): number {
  const prev = list[i - 1]!.sort_order;
  const prev2 = list[i - 2]?.sort_order ?? prev - 2000;
  return (prev + prev2) / 2;
}
function after(list: { sort_order: number }[], i: number): number {
  const next = list[i + 1]!.sort_order;
  const next2 = list[i + 2]?.sort_order ?? next + 2000;
  return (next + next2) / 2;
}

function NameColorDialog({ open, title, label, onClose, submit, onDone, parents }: {
  open: boolean; title: string; label: string; onClose: () => void;
  submit: (name: string, color: string, parent: string | null) => Promise<unknown>; onDone: () => void; parents?: Label[];
}) {
  const t = useT();
  const [name, setName] = useState("");
  const [color, setColor] = useState(PALETTE[7]!);
  const [parent, setParent] = useState("");
  const m = useMutation({
    mutationFn: () => submit(name, color, parent || null),
    onSuccess: () => {
      onDone();
      setName("");
      onClose();
    },
  });
  return (
    <Dialog open={open} onClose={onClose} title={title}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); m.mutate(); }}>
        <ErrorAlert message={m.isError ? errorText(m.error, t) : null} />
        <TextField label={label} value={name} onChange={(e) => setName(e.target.value)} required maxLength={50} autoFocus />
        <div className="field">
          <label>{t("proj.cover")}</label>
          <ColorPicker label={t("proj.cover")} value={color} onChange={setColor} />
        </div>
        {parents ? (
          <div className="field">
            <label htmlFor="lp">{t("label.parent")}</label>
            <Select id="lp" value={parent} onChange={(v) => setParent(v)}>
              <option value="">{t("label.noParent")}</option>
              {parents.map((l) => (
                <option key={l.id} value={l.id}>{l.name}</option>
              ))}
            </Select>
          </div>
        ) : null}
        <div>
          <button className="btn btn-primary" disabled={m.isPending || !name.trim()}>{t("common.add")}</button>
        </div>
      </form>
    </Dialog>
  );
}

function Labels({ p }: { p: Project }) {
  const t = useT();
  const ws = useWs();
  const qc = useQueryClient();
  const labels = useLabels(ws.slug, p.id);
  const editable = pcan.editSettings(p);
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const url = `${projectPath(ws.slug, p.id)}/labels`;
  const refresh = () => qc.invalidateQueries({ queryKey: pq.labels(ws.slug, p.id) });
  const upd = useMutation({
    mutationFn: (v: { id: string; body: Record<string, unknown> }) => patch<Label[]>(`${url}/${v.id}`, v.body),
    onSuccess: (all) => qc.setQueryData(pq.labels(ws.slug, p.id), all),
    onError: (e) => setError(errorText(e, t)),
  });
  const remove = useMutation({ mutationFn: (id: string) => del<void>(`${url}/${id}`), onSuccess: refresh });
  const all = labels.data ?? [];
  const top = all.filter((l) => !l.parent_id);
  const ordered = top.flatMap((l) => [l, ...all.filter((c) => c.parent_id === l.id)]);
  return (
    <>
      <div className="page-head">
        <span className="grow" />
        {editable ? <button className="btn btn-primary" onClick={() => setAdding(true)}>{t("label.add")}</button> : null}
      </div>
      <ErrorAlert message={error} />
      <section className="card rows">
        {ordered.length === 0 ? <div className="row muted">—</div> : null}
        {ordered.map((l) => (
          <div className="row" key={l.id} style={{ paddingLeft: l.parent_id ? 40 : undefined }}>
            <input type="color" value={l.color.length === 7 ? l.color : "#888888"} disabled={!editable} className="color-input" aria-label={t("proj.cover")} onChange={(e) => upd.mutate({ id: l.id, body: { color: e.target.value } })} />
            <input className="inline-input" defaultValue={l.name} disabled={!editable} aria-label={t("label.name")} onBlur={(e) => e.target.value.trim() && e.target.value !== l.name && upd.mutate({ id: l.id, body: { name: e.target.value } })} />
            {editable ? <button className="icon-btn" aria-label={t("common.delete")} onClick={() => remove.mutate(l.id)}><X /></button> : null}
          </div>
        ))}
      </section>
      <NameColorDialog
        open={adding}
        title={t("label.add")}
        label={t("label.name")}
        onClose={() => setAdding(false)}
        submit={(name, color, parent) => post<Label>(url, { name, color, parent_id: parent })}
        onDone={refresh}
        parents={top}
      />
    </>
  );
}

function Estimates({ p }: { p: Project }) {
  const t = useT();
  const save = useProjectPatch(p);
  const editable = pcan.editSettings(p);
  const [type, setType] = useState<EstimateType>(p.estimate_type);
  const [values, setValues] = useState(p.estimate_values.join(", "));
  const defaults: Record<EstimateType, string> = { none: "", points: "1, 2, 3, 5, 8, 13", tshirt: "XS, S, M, L, XL, XXL" };
  return (
    <section className="card">
      <form className="card-body form" onSubmit={(e) => { e.preventDefault(); save.mutate({ estimate_type: type, estimate_values: values.split(",").map((v) => v.trim()).filter(Boolean) }); }}>
        <ErrorAlert message={save.isError ? errorText(save.error, t) : null} />
        {save.isSuccess ? <div className="alert alert-success">{t("common.saved")}</div> : null}
        <fieldset disabled={!editable} className="form" style={{ border: 0, padding: 0, margin: 0 }}>
          <div className="field">
            <label>{t("est.type")}</label>
            {(["none", "points", "tshirt"] as EstimateType[]).map((k) => (
              <label key={k} className="checkbox">
                <input type="radio" name="est" checked={type === k} onChange={() => { setType(k); setValues(k === p.estimate_type ? p.estimate_values.join(", ") : defaults[k]); }} />
                <span>{t(`est.${k}`)}</span>
              </label>
            ))}
          </div>
          {type !== "none" ? (
            <TextField label={t("est.values")} hint={t("est.valuesHint")} value={values} onChange={(e) => setValues(e.target.value)} />
          ) : null}
        </fieldset>
        {editable ? <div><button className="btn btn-primary" disabled={save.isPending}>{t("common.save")}</button></div> : null}
      </form>
    </section>
  );
}

function Features({ p }: { p: Project }) {
  const t = useT();
  const save = useProjectPatch(p);
  const editable = pcan.editSettings(p);
  const items = [
    ["feature_cycles", "feat.cycles", "feat.cyclesHint"],
    ["feature_modules", "feat.modules", "feat.modulesHint"],
    ["feature_pages", "feat.pages", "feat.pagesHint"],
    ["feature_intake", "feat.intake", "feat.intakeHint"],
  ] as const;
  // Local state so the switch flips in the same frame; the server answer then confirms it
  // (or the optimistic rollback in useProjectPatch restores it).
  const [on, setOn] = useState({ feature_cycles: p.feature_cycles, feature_modules: p.feature_modules, feature_pages: p.feature_pages, feature_intake: p.feature_intake });
  return (
    <section className="card rows">
      <ErrorAlert message={save.isError ? errorText(save.error, t) : null} />
      {items.map(([key, label, hint]) => (
        <label className="row checkbox" key={key}>
          <div className="row-main">
            <b>{t(label)}</b>
            <span className="muted small">{t(hint)}</span>
          </div>
          <input
            type="checkbox"
            className="switch"
            checked={on[key]}
            disabled={!editable}
            onChange={(e) => {
              const v = e.target.checked;
              setOn((o) => ({ ...o, [key]: v }));
              save.mutate({ [key]: v }, { onError: () => setOn((o) => ({ ...o, [key]: !v })) });
            }}
          />
        </label>
      ))}
    </section>
  );
}
