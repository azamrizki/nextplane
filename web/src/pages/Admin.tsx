import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Navigate } from "react-router";
import { get, patch, post } from "../api/client";
import type { AdminSettings, AdminWorkspace, SignupPolicy, User } from "../api/types";
import { Avatar } from "../components/Avatar";
import { Dialog } from "../components/Dialog";
import { PlainPage } from "../components/Layout";
import { ErrorAlert, TextField } from "../components/ui";
import { useT } from "../i18n";
import { errorText } from "../lib/errors";
import { qk, useMe } from "../lib/session";
import { LinkBox } from "./Members";
import { AuthSection } from "./AdminAuth";

export function AdminPage() {
  const t = useT();
  const me = useMe().data!;
  const qc = useQueryClient();
  const settings = useQuery({ queryKey: ["admin", "settings"], queryFn: () => get<AdminSettings>("/admin/settings"), enabled: me.is_instance_admin });
  const users = useQuery({ queryKey: ["admin", "users"], queryFn: () => get<User[]>("/admin/users"), enabled: me.is_instance_admin });
  const workspaces = useQuery({ queryKey: ["admin", "workspaces"], queryFn: () => get<AdminWorkspace[]>("/admin/workspaces"), enabled: me.is_instance_admin });
  const [name, setName] = useState<string | null>(null);
  const [reset, setReset] = useState<{ email: string; link: string } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: (body: { name?: string; signup_policy?: SignupPolicy }) => patch<AdminSettings>("/admin/settings", body),
    onSuccess: (s) => {
      qc.setQueryData(["admin", "settings"], s);
      qc.invalidateQueries({ queryKey: qk.instance });
    },
    onError: (e) => setError(errorText(e, t)),
  });
  const toggleAdmin = useMutation({
    mutationFn: (v: { id: string; on: boolean }) => patch<User>(`/admin/users/${v.id}`, { is_instance_admin: v.on }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["admin", "users"] }),
    onError: (e) => setError(errorText(e, t)),
  });
  const makeReset = useMutation({
    mutationFn: (u: User) => post<{ link: string }>(`/admin/users/${u.id}/reset-link`).then((r) => ({ email: u.email, link: r.link })),
    onSuccess: setReset,
    onError: (e) => setError(errorText(e, t)),
  });

  if (!me.is_instance_admin) return <Navigate to="/" replace />;
  const s = settings.data;
  return (
    <PlainPage back>
      <h1>{t("admin.title")}</h1>
      <ErrorAlert message={error} />
      {s ? (
        <section className="card">
          <div className="card-head">{t("admin.settings")}</div>
          <div className="card-body form">
            <form
              className="link-box"
              onSubmit={(e) => {
                e.preventDefault();
                if (name !== null) save.mutate({ name });
              }}
            >
              <div className="grow">
                <TextField label={t("setup.instanceName")} value={name ?? s.name} onChange={(e) => setName(e.target.value)} maxLength={80} />
              </div>
              <button className="btn" style={{ alignSelf: "flex-end" }} disabled={name === null || save.isPending}>{t("common.save")}</button>
            </form>
            <div className="field">
              <label>{t("admin.signupPolicy")}</label>
              {(["invite", "open", "closed"] as SignupPolicy[]).map((p) => (
                <label key={p} className="checkbox">
                  <input type="radio" name="policy" checked={s.signup_policy === p} onChange={() => save.mutate({ signup_policy: p })} />
                  <span>{t(`admin.policy.${p}`)}</span>
                </label>
              ))}
            </div>
            <div className="field">
              <label>{t("admin.email")}</label>
              <span className={s.email_enabled ? "" : "muted small"}>{s.email_enabled ? t("admin.emailOn") : t("admin.emailOff")}</span>
            </div>
          </div>
        </section>
      ) : null}
      <AuthSection />

      <section className="card">
        <div className="card-head">{t("admin.users")}</div>
        <div className="rows">
          {(users.data ?? []).map((u) => (
            <div className="row" key={u.id}>
              <Avatar name={u.name} email={u.email} src={u.avatar} size={28} />
              <div className="row-main">
                <span className="ellipsis">{u.name || u.email}</span>
                <span className="faint small ellipsis">{u.email}</span>
              </div>
              <label className="checkbox small">
                <input type="checkbox" checked={u.is_instance_admin} onChange={(e) => toggleAdmin.mutate({ id: u.id, on: e.target.checked })} />
                {t("admin.isAdmin")}
              </label>
              <button className="btn btn-sm" onClick={() => makeReset.mutate(u)}>{t("admin.resetLink")}</button>
            </div>
          ))}
        </div>
      </section>

      <section className="card">
        <div className="card-head">{t("admin.workspaces")}</div>
        <div className="rows">
          {(workspaces.data ?? []).map((w) => (
            <div className="row" key={w.id}>
              <div className="row-main">
                <span>{w.name}</span>
                <span className="faint small">/{w.slug} · {t("admin.memberCount", { n: w.member_count })}{w.deleted_at ? ` · ${t("admin.inTrash")}` : ""}</span>
              </div>
            </div>
          ))}
        </div>
      </section>

      <Dialog open={!!reset} onClose={() => setReset(null)} title={t("admin.resetLink")} width={520}>
        {reset ? (
          <div className="form">
            <p className="muted" style={{ margin: 0 }}>{t("admin.resetLinkBody", { email: reset.email })}</p>
            <LinkBox link={reset.link} />
          </div>
        ) : null}
      </Dialog>
    </PlainPage>
  );
}
