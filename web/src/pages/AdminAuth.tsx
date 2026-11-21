import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { KeyRound, LogIn } from "lucide-react";
import { useState } from "react";
import { get, patch } from "../api/client";
import { Dialog } from "../components/Dialog";
import { ErrorAlert, TextField } from "../components/ui";
import { useT } from "../i18n";
import { errorText } from "../lib/errors";
import { qk } from "../lib/session";
import { toast } from "../lib/toast";
import { LinkBox } from "./Members";

interface ProviderOut {
  key: "google" | "github" | "gitlab" | "gitea";
  name: string;
  enabled: boolean;
  configured: boolean;
  client_id: string;
  has_secret: boolean;
  host: string;
  org: string;
  callback_url: string;
}
interface AuthSettings {
  password_enabled: boolean;
  config_sso: string | null;
  providers: ProviderOut[];
}

const KEY = ["admin", "auth"];

/** Instance admin › Sign-in methods (as in Plane's Authentication page): passwords and the
 * Google, GitHub, GitLab and Gitea buttons. At least one way in must stay on. */
export function AuthSection() {
  const t = useT();
  const qc = useQueryClient();
  const auth = useQuery({ queryKey: KEY, queryFn: () => get<AuthSettings>("/admin/auth") });
  const [editing, setEditing] = useState<ProviderOut | null>(null);
  // Switches flip at once; the server's answer then confirms (or undoes) them.
  const [flip, setFlip] = useState<Record<string, boolean>>({});
  const save = useMutation({
    mutationFn: (body: Record<string, unknown>) => patch<AuthSettings>("/admin/auth", body),
    onSuccess: (s) => {
      qc.setQueryData(KEY, s);
      qc.invalidateQueries({ queryKey: qk.instance });
    },
    onError: (e) => toast({ kind: "error", title: errorText(e, t) }),
    onSettled: () => setFlip({}),
  });
  const toggle = (key: string, on: boolean, body: Record<string, unknown>) => {
    setFlip((f) => ({ ...f, [key]: on }));
    save.mutate(body);
  };
  const a = auth.data;
  if (!a) return null;
  return (
    <section className="card">
      <div className="card-head">{t("adm.auth")}</div>
      <div className="rows">
        <label className="row checkbox">
          <span className="auth-icon" aria-hidden>
            <KeyRound />
          </span>
          <div className="row-main">
            <b>{t("adm.passwords")}</b>
            <span className="muted small">{t("adm.passwordsHint")}</span>
          </div>
          <input type="checkbox" role="switch" className="switch" aria-label={t("adm.passwords")} checked={flip.password ?? a.password_enabled} onChange={(e) => toggle("password", e.target.checked, { password_enabled: e.target.checked })} />
        </label>
        {a.config_sso ? (
          <div className="row">
            <span className="auth-icon" aria-hidden>
              <LogIn />
            </span>
            <div className="row-main">
              <b>{a.config_sso}</b>
              <span className="muted small">{t("adm.configSso")}</span>
            </div>
          </div>
        ) : null}
        {a.providers.map((p) => (
          <div className="row" key={p.key}>
            <span className="auth-icon" aria-hidden>
              <LogIn />
            </span>
            <div className="row-main">
              <b>{p.name}</b>
              <span className="muted small">{t(`adm.p.${p.key}`)}</span>
            </div>
            <button className="btn btn-sm" onClick={() => setEditing(p)}>
              {p.configured ? t("common.edit") : t("adm.configure")}
            </button>
            {p.configured ? (
              <input type="checkbox" role="switch" className="switch" aria-label={t("adm.useProvider", { name: p.name })} checked={flip[p.key] ?? p.enabled} onChange={(e) => toggle(p.key, e.target.checked, { providers: { [p.key]: { enabled: e.target.checked } } })} />
            ) : null}
          </div>
        ))}
      </div>
      {editing ? <ProviderDialog p={editing} onClose={() => setEditing(null)} /> : null}
    </section>
  );
}

function ProviderDialog({ p, onClose }: { p: ProviderOut; onClose: () => void }) {
  const t = useT();
  const qc = useQueryClient();
  const [f, setF] = useState({ client_id: p.client_id, client_secret: "", host: p.host || (p.key === "gitlab" ? "https://gitlab.com" : p.key === "gitea" ? "https://gitea.com" : ""), org: p.org });
  const needsHost = p.key === "gitlab" || p.key === "gitea";
  const save = useMutation({
    mutationFn: () => {
      const body: Record<string, unknown> = { client_id: f.client_id, client_secret: f.client_secret };
      if (needsHost || p.key === "github") body.host = f.host;
      if (p.key === "github") body.org = f.org;
      return patch<AuthSettings>("/admin/auth", { providers: { [p.key]: body } });
    },
    onSuccess: (s) => {
      qc.setQueryData(KEY, s);
      toast({ kind: "success", title: t("adm.providerSaved", { name: p.name }), body: t("adm.providerTest") });
      onClose();
    },
  });
  const ready = f.client_id.trim() && (f.client_secret.trim() || p.has_secret) && (!needsHost || f.host.trim());
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("adm.setUp", { name: p.name })}
      width={520}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button className="btn btn-primary" disabled={!ready || save.isPending} onClick={() => save.mutate()}>
            {t("common.save")}
          </button>
        </>
      }
    >
      <ErrorAlert message={save.isError ? errorText(save.error, t) : null} />
      <div className="form">
        {needsHost ? <TextField label={t("adm.host")} value={f.host} onChange={(e) => setF({ ...f, host: e.target.value })} placeholder="https://" /> : null}
        <TextField label={t("adm.clientId")} value={f.client_id} onChange={(e) => setF({ ...f, client_id: e.target.value })} autoComplete="off" />
        <TextField
          label={t("adm.clientSecret")}
          type="password"
          value={f.client_secret}
          onChange={(e) => setF({ ...f, client_secret: e.target.value })}
          placeholder={p.has_secret ? t("adm.secretKept") : ""}
          autoComplete="new-password"
        />
        {p.key === "github" ? (
          <>
            <TextField label={t("adm.githubOrg")} hint={t("adm.githubOrgHint")} value={f.org} onChange={(e) => setF({ ...f, org: e.target.value })} />
            <TextField label={t("adm.githubHost")} hint={t("adm.githubHostHint")} value={f.host} onChange={(e) => setF({ ...f, host: e.target.value })} placeholder="https://github.com" />
          </>
        ) : null}
        <div className="field">
          <label>{t("adm.callback")}</label>
          <LinkBox link={p.callback_url} />
          <span className="hint">{t("adm.callbackHint", { name: p.name })}</span>
        </div>
      </div>
    </Dialog>
  );
}

interface ServicesOut {
  ai: { enabled: boolean; has_key: boolean; model: string; base_url: string };
  unsplash: { enabled: boolean };
}

/** Instance admin › AI assistant and Unsplash (as in Plane's "AI" and "Images" pages). Keys
 * are saved here and never shown again. */
export function ServicesSection() {
  const t = useT();
  const qc = useQueryClient();
  const services = useQuery({ queryKey: ["admin", "services"], queryFn: () => get<ServicesOut>("/admin/services") });
  const [ai, setAi] = useState<{ api_key: string; model: string; base_url: string } | null>(null);
  const [unsplash, setUnsplash] = useState("");
  const save = useMutation({
    mutationFn: (body: Record<string, unknown>) => patch<ServicesOut>("/admin/services", body),
    onSuccess: (s) => {
      qc.setQueryData(["admin", "services"], s);
      qc.invalidateQueries({ queryKey: qk.instance });
      setAi(null);
      setUnsplash("");
      toast({ kind: "success", title: t("common.saved") });
    },
    onError: (e) => toast({ kind: "error", title: errorText(e, t) }),
  });
  const s = services.data;
  if (!s) return null;
  const form = ai ?? { api_key: "", model: s.ai.model, base_url: s.ai.base_url };
  return (
    <section className="card">
      <div className="card-head">{t("adm.services")}</div>
      <div className="card-body form">
        <h3 className="section-title">{t("adm.ai")}</h3>
        <p className="muted small">{t("adm.aiHint")}</p>
        <TextField
          label={t("adm.aiKey")}
          type="password"
          autoComplete="new-password"
          placeholder={s.ai.has_key ? t("adm.secretKept") : "sk-…"}
          value={form.api_key}
          onChange={(e) => setAi({ ...form, api_key: e.target.value })}
        />
        <TextField label={t("adm.aiModel")} value={form.model} onChange={(e) => setAi({ ...form, model: e.target.value })} />
        <TextField label={t("adm.aiBase")} hint={t("adm.aiBaseHint")} value={form.base_url} onChange={(e) => setAi({ ...form, base_url: e.target.value })} />
        <div className="row-flex">
          <button className="btn btn-primary" disabled={!ai || save.isPending} onClick={() => save.mutate({ ai: { ...form, api_key: form.api_key || undefined } })}>
            {t("common.save")}
          </button>
          {s.ai.has_key ? (
            <button className="btn btn-ghost" onClick={() => save.mutate({ ai: { api_key: "" } })}>
              {t("adm.removeKey")}
            </button>
          ) : null}
          <span className={s.ai.enabled ? "small" : "muted small"}>{s.ai.enabled ? t("adm.aiOn") : t("adm.off")}</span>
        </div>
        <h3 className="section-title">{t("adm.unsplash")}</h3>
        <p className="muted small">{t("adm.unsplashHint")}</p>
        <TextField
          label={t("adm.unsplashKey")}
          type="password"
          autoComplete="new-password"
          placeholder={s.unsplash.enabled ? t("adm.secretKept") : ""}
          value={unsplash}
          onChange={(e) => setUnsplash(e.target.value)}
        />
        <div className="row-flex">
          <button className="btn btn-primary" disabled={!unsplash.trim() || save.isPending} onClick={() => save.mutate({ unsplash: { access_key: unsplash } })}>
            {t("common.save")}
          </button>
          {s.unsplash.enabled ? (
            <button className="btn btn-ghost" onClick={() => save.mutate({ unsplash: { access_key: "" } })}>
              {t("adm.removeKey")}
            </button>
          ) : null}
          <span className={s.unsplash.enabled ? "small" : "muted small"}>{s.unsplash.enabled ? t("adm.unsplashOn") : t("adm.off")}</span>
        </div>
      </div>
    </section>
  );
}
