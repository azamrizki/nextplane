import { useMutation, useQueryClient } from "@tanstack/react-query";
import { WsSettingsFrame } from "../components/SettingsFrame";
import { useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router";
import { del, patch, post } from "../api/client";
import type { MyWorkspace } from "../api/types";
import { ConfirmDialog } from "../components/Dialog";
import { ErrorAlert, TextField } from "../components/ui";
import { uploadFile } from "../components/Collab";
import { Bots } from "../components/Tokens";
import { useT } from "../i18n";
import { errorText } from "../lib/errors";
import { can } from "../lib/perm";
import { wq } from "../lib/queries";
import { useWs } from "./WorkspaceHome";

export function WorkspaceSettingsPage() {
  const t = useT();
  const ws = useWs();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const editable = can.editWorkspace(ws);
  const [form, setForm] = useState({ name: ws.name, slug: ws.slug, logo: ws.logo ?? "", members_can_create_projects: ws.members_can_create_projects });
  const [saved, setSaved] = useState(false);
  const [confirm, setConfirm] = useState<"delete" | "leave" | null>(null);

  const save = useMutation({
    mutationFn: () => patch<MyWorkspace>(`/workspaces/${ws.slug}`, { ...form, logo: form.logo.trim() || null }),
    onSuccess: async (updated) => {
      setSaved(true);
      await qc.invalidateQueries({ queryKey: wq.list });
      if (updated.slug !== ws.slug) navigate(`/${updated.slug}/settings`, { replace: true });
    },
  });
  const remove = useMutation({
    mutationFn: () => del<void>(`/workspaces/${ws.slug}`),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: wq.list });
      navigate("/", { replace: true });
    },
  });
  const leave = useMutation({
    mutationFn: () => post<void>(`/workspaces/${ws.slug}/leave`),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: wq.list });
      navigate("/", { replace: true });
    },
  });

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    setSaved(false);
    save.mutate();
  };

  return (
    <WsSettingsFrame ws={ws} crumbs={[{ label: t("ws.settings.general") }]}>
      <section className="card">
        <div className="card-head">{t("ws.settings.general")}</div>
        <form className="card-body form" onSubmit={onSubmit}>
          <ErrorAlert message={save.isError ? errorText(save.error, t) : null} />
          {saved ? <div className="alert alert-success">{t("common.saved")}</div> : null}
          <fieldset disabled={!editable} style={{ border: 0, padding: 0, margin: 0 }} className="form">
            <TextField label={t("ws.name")} value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} required maxLength={80} />
            <TextField label={t("ws.slug")} value={form.slug} onChange={(e) => setForm({ ...form, slug: e.target.value.toLowerCase() })} required minLength={3} maxLength={48} hint={t("ws.slugHint")} />
            <div className="form-row" style={{ alignItems: "flex-end" }}>
              <div className="grow">
                <TextField label={`${t("ws.logo")} (${t("common.optional")})`} value={form.logo} onChange={(e) => setForm({ ...form, logo: e.target.value })} placeholder="https://…" />
              </div>
              <label className="btn">
                {t("ws.uploadLogo")}
                <input type="file" accept="image/png,image/jpeg,image/gif,image/webp" hidden onChange={async (e) => {
                  const f = e.target.files?.[0];
                  if (!f) return;
                  const r = await uploadFile<{ logo: string }>(`/workspaces/${ws.slug}/logo`, f);
                  setForm((x) => ({ ...x, logo: r.logo }));
                  qc.invalidateQueries({ queryKey: wq.list });
                }} />
              </label>
            </div>
            <label className="checkbox">
              <input type="checkbox" checked={form.members_can_create_projects} onChange={(e) => setForm({ ...form, members_can_create_projects: e.target.checked })} />
              <span>
                {t("ws.membersCanCreate")}
                <br />
                <span className="faint small">{t("ws.membersCanCreateHint")}</span>
              </span>
            </label>
          </fieldset>
          {editable ? (
            <div>
              <button className="btn btn-primary" disabled={save.isPending}>
                {t("common.save")}
              </button>
            </div>
          ) : null}
        </form>
      </section>

      {can.invite(ws) ? <Bots ws={ws} /> : null}

      {can.editWorkspace(ws) ? (
        <section className="card">
          <div className="card-head">{t("ws.integrations")}</div>
          <div className="rows">
            <Link className="row row-link" to={`/${ws.slug}/settings/webhooks`}>
              <div className="row-main">
                <b>{t("ws.webhooksLink")}</b>
                <span className="muted small">{t("ws.webhooksHint")}</span>
              </div>
            </Link>
            <Link className="row row-link" to={`/${ws.slug}/settings/github`}>
              <div className="row-main">
                <b>{t("ws.githubLink")}</b>
                <span className="muted small">{t("ws.githubHint")}</span>
              </div>
            </Link>
          </div>
        </section>
      ) : null}

      <section className="card card-danger">
        <div className="card-head">{t("ws.danger")}</div>
        <div className="rows">
          <div className="row">
            <div className="row-main">
              <b>{t("ws.leave")}</b>
              <span className="muted small">{t("ws.leaveBody")}</span>
            </div>
            <button className="btn" onClick={() => setConfirm("leave")}>
              {t("ws.leave")}
            </button>
          </div>
          {can.deleteWorkspace(ws) ? (
            <div className="row">
              <div className="row-main">
                <b>{t("ws.delete")}</b>
                <span className="muted small">{t("ws.deleteBody")}</span>
              </div>
              <button className="btn btn-danger-outline" onClick={() => setConfirm("delete")}>
                {t("ws.delete")}
              </button>
            </div>
          ) : null}
        </div>
        <div className="card-body" style={{ paddingTop: 0 }}>
          <ErrorAlert message={leave.isError ? errorText(leave.error, t) : remove.isError ? errorText(remove.error, t) : null} />
        </div>
      </section>

      <ConfirmDialog
        open={confirm === "delete"}
        onClose={() => setConfirm(null)}
        onConfirm={() => {
          setConfirm(null);
          remove.mutate();
        }}
        title={t("ws.delete")}
        body={t("ws.deleteBody")}
        confirmLabel={t("common.delete")}
        danger
        typeToConfirm={{ label: t("ws.deleteConfirm", { slug: ws.slug }), value: ws.slug }}
        busy={remove.isPending}
      />
      <ConfirmDialog
        open={confirm === "leave"}
        onClose={() => setConfirm(null)}
        onConfirm={() => {
          setConfirm(null);
          leave.mutate();
        }}
        title={t("ws.leave")}
        body={t("ws.leaveBody")}
        confirmLabel={t("ws.leave")}
        danger
        busy={leave.isPending}
      />
    </WsSettingsFrame>
  );
}
