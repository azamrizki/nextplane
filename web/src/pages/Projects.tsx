import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Briefcase, Lock, UsersRound } from "lucide-react";
import { PageHeader } from "../components/PageHeader";
import { useEffect, useState, type FormEvent } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { post } from "../api/client";
import type { Project } from "../api/types";
import { ColorPicker } from "../components/ColorPicker";
import { Dialog } from "../components/Dialog";
import { ErrorAlert, TextField } from "../components/ui";
import { useT } from "../i18n";
import { errorText } from "../lib/errors";
import { can, suggestIdentifier } from "../lib/perm";
import { PALETTE, pq, projectPath, useProjects } from "../lib/projects";
import { useWs } from "./WorkspaceHome";

export function ProjectIcon({ p, size = 20 }: { p: Pick<Project, "icon" | "name" | "cover_color">; size?: number }) {
  if (p.icon) return <span className="proj-icon" style={{ fontSize: size * 0.8, width: size, height: size }}>{p.icon}</span>;
  return (
    <span className="proj-icon proj-icon-letter" style={{ background: p.cover_color ?? "#8b8d98", width: size, height: size, fontSize: size * 0.5 }}>
      {(p.name.trim()[0] ?? "?").toUpperCase()}
    </span>
  );
}

export function ProjectsPage() {
  const t = useT();
  const ws = useWs();
  const [archived, setArchived] = useState(false);
  const list = useProjects(ws.slug, archived);
  const [params, setParams] = useSearchParams();
  const [creating, setCreating] = useState(() => params.get("new") === "1");
  useEffect(() => {
    if (params.get("new") === "1") {
      setCreating(true);
      setParams({}, { replace: true });
    }
  }, [params, setParams]);
  const qc = useQueryClient();
  const navigate = useNavigate();
  const join = useMutation({
    mutationFn: (id: string) => post<Project>(projectPath(ws.slug, id) + "/join"),
    onSuccess: (p) => {
      qc.invalidateQueries({ queryKey: pq.all(ws.slug) });
      navigate(`/${ws.slug}/projects/${p.id}`);
    },
  });
  const projects = list.data ?? [];
  return (
    <div className="page page-wide">
      <PageHeader
        crumbs={[{ icon: <Briefcase />, label: t("proj.title") }]}
        extra={list.isSuccess ? <span className="badge count-chip">{projects.length}</span> : null}
        actions={
          <>
            <div className="seg" role="tablist">
              <button role="tab" aria-selected={!archived} className={!archived ? "on" : ""} onClick={() => setArchived(false)}>{t("proj.active")}</button>
              <button role="tab" aria-selected={archived} className={archived ? "on" : ""} onClick={() => setArchived(true)}>{t("proj.archived")}</button>
            </div>
            {can.createProject(ws) ? (
              <button className="btn btn-primary" onClick={() => setCreating(true)}>{t("proj.new")}</button>
            ) : null}
          </>
        }
      />
      <ErrorAlert message={join.isError ? errorText(join.error, t) : null} />
      {list.isSuccess && projects.length === 0 ? <div className="card empty">{archived ? t("proj.emptyArchived") : t("proj.empty")}</div> : null}
      <div className="proj-grid">
        {projects.map((p) => (
          <div key={p.id} className="proj-card">
            <div className="proj-card-cover" style={{ ["--cover" as string]: p.cover_color ?? "#8b8d98" }}>
              <span className="logo-tile proj-card-logo">
                <ProjectIcon p={p} size={20} />
              </span>
              {p.my_role ? <span className="badge proj-card-role">{t(`prole.${p.my_role}`)}</span> : null}
            </div>
            <div className="proj-card-body">
              {p.my_role ? (
                <Link to={`/${ws.slug}/projects/${p.id}`} className="proj-name stretched">{p.name}</Link>
              ) : (
                <span className="proj-name">{p.name}</span>
              )}
              <span className="faint small">
                {p.identifier}
                {p.is_private ? <> · <Lock className="inline-icon" aria-label={t("proj.private")} /></> : ""}
              </span>
              {p.description ? <p className="muted small proj-desc">{p.description}</p> : null}
              <div className="proj-card-foot">
                <span className="faint small">
                  <UsersRound className="inline-icon" /> {t("proj.members", { n: p.member_count })}
                </span>
                {!p.is_member && !p.my_role ? (
                  <button className="btn btn-sm proj-card-join" onClick={() => join.mutate(p.id)} disabled={join.isPending}>{t("proj.join")}</button>
                ) : null}
              </div>
            </div>
          </div>
        ))}
      </div>
      <CreateProjectDialog open={creating} onClose={() => setCreating(false)} />
    </div>
  );
}

function CreateProjectDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  const ws = useWs();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const empty = { name: "", identifier: "", description: "", icon: "", cover_color: PALETTE[6]!, is_private: false };
  const [f, setF] = useState(empty);
  const [idEdited, setIdEdited] = useState(false);
  const identifier = idEdited ? f.identifier : suggestIdentifier(f.name);
  const create = useMutation({
    mutationFn: () => post<Project>(projectPath(ws.slug), { ...f, identifier, icon: f.icon || null }),
    onSuccess: (p) => {
      qc.invalidateQueries({ queryKey: pq.all(ws.slug) });
      setF(empty);
      setIdEdited(false);
      onClose();
      navigate(`/${ws.slug}/projects/${p.id}`);
    },
  });
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    create.mutate();
  };
  return (
    <Dialog open={open} onClose={onClose} title={t("proj.create")} width={520}>
      <form className="form" onSubmit={onSubmit}>
        <ErrorAlert message={create.isError ? errorText(create.error, t) : null} />
        <div className="form-row">
          <div style={{ width: 72 }}>
            <TextField label={t("proj.icon")} value={f.icon} onChange={(e) => setF({ ...f, icon: e.target.value })} maxLength={8} placeholder="🚀" />
          </div>
          <div className="grow">
            <TextField label={t("proj.name")} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} required maxLength={80} autoFocus />
          </div>
          <div style={{ width: 120 }}>
            <TextField
              label={t("proj.identifier")}
              value={identifier}
              onChange={(e) => {
                setIdEdited(true);
                setF({ ...f, identifier: e.target.value.toUpperCase() });
              }}
              required
              maxLength={12}
            />
          </div>
        </div>
        <span className="hint faint small" style={{ marginTop: -8 }}>{t("proj.identifierHint", { id: identifier || "WEB" })}</span>
        <div className="field">
          <label htmlFor="proj-desc">{t("proj.description")} ({t("common.optional")})</label>
          <textarea id="proj-desc" className="textarea" value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} maxLength={4000} />
        </div>
        <div className="field">
          <label>{t("proj.cover")}</label>
          <ColorPicker label={t("proj.cover")} value={f.cover_color} onChange={(c) => setF({ ...f, cover_color: c })} />
        </div>
        <label className="checkbox">
          <input type="checkbox" checked={f.is_private} onChange={(e) => setF({ ...f, is_private: e.target.checked })} />
          <span>
            {t("proj.private")}
            <br />
            <span className="faint small">{f.is_private ? t("proj.privateHint") : t("proj.publicHint")}</span>
          </span>
        </label>
        <div>
          <button className="btn btn-primary" disabled={create.isPending || !f.name.trim()}>{t("proj.create")}</button>
        </div>
      </form>
    </Dialog>
  );
}
