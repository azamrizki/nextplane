import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { get, patch, post } from "../api/client";
import type { User, Workspace } from "../api/types";
import { PlainPage } from "../components/Layout";
import { Avatar } from "../components/Avatar";
import { uploadFile } from "../components/Collab";
import { ApiTokens } from "../components/Tokens";
import { ErrorAlert, TextField } from "../components/ui";
import { LANGS, useI18n, type Lang } from "../i18n";
import { errorText } from "../lib/errors";
import { wq } from "../lib/queries";
import { qk, useMe } from "../lib/session";
import { useTheme, type ThemeChoice } from "../lib/theme";
import { Select } from "../components/Select";

export function ProfilePage() {
  const { t, lang, setLang } = useI18n();
  const me = useMe().data!;
  const qc = useQueryClient();
  const [theme, setTheme] = useTheme();
  const [name, setName] = useState(me.name);
  const [pw, setPw] = useState({ current: "", new: "" });
  const [avatarError, setAvatarError] = useState<string | null>(null);

  const saveName = useMutation({
    mutationFn: () => patch<User>("/me", { name }),
    onSuccess: (u) => qc.setQueryData(qk.me, u),
  });
  const changePw = useMutation({
    mutationFn: () => post<void>("/me/password", pw),
    onSuccess: () => setPw({ current: "", new: "" }),
  });
  const trash = useQuery({ queryKey: wq.trash, queryFn: () => get<Workspace[]>("/workspaces/trash") });
  const restore = useMutation({
    mutationFn: (slug: string) => post<void>(`/workspaces/${slug}/restore`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: wq.trash });
      qc.invalidateQueries({ queryKey: wq.list });
    },
  });

  return (
    <PlainPage back>
      <h1>{t("profile.title")}</h1>
      <section className="card">
        <form
          className="card-body form"
          onSubmit={(e: FormEvent) => {
            e.preventDefault();
            saveName.mutate();
          }}
        >
          <ErrorAlert message={saveName.isError ? errorText(saveName.error, t) : null} />
          {saveName.isSuccess ? <div className="alert alert-success">{t("common.saved")}</div> : null}
          <div className="field">
            <label>{t("profile.avatar")}</label>
            <div className="avatar-edit">
              <Avatar name={me.name} email={me.email} src={me.avatar} size={48} />
              <label className="btn btn-sm">
                {t("profile.uploadAvatar")}
                <input type="file" accept="image/png,image/jpeg,image/gif,image/webp" hidden onChange={async (e) => {
                  const f = e.target.files?.[0];
                  if (!f) return;
                  try {
                    qc.setQueryData(qk.me, await uploadFile<User>("/me/avatar", f));
                  } catch (err) {
                    setAvatarError(errorText(err, t));
                  }
                }} />
              </label>
              {me.avatar ? <button type="button" className="btn btn-sm btn-ghost" onClick={async () => qc.setQueryData(qk.me, await patch<User>("/me", { avatar: null }))}>{t("profile.removeAvatar")}</button> : null}
            </div>
            <ErrorAlert message={avatarError} />
          </div>
          <TextField label={t("profile.email")} value={me.email} disabled />
          <TextField label={t("profile.name")} value={name} onChange={(e) => setName(e.target.value)} maxLength={80} />
          <div>
            <button className="btn btn-primary" disabled={saveName.isPending}>{t("common.save")}</button>
          </div>
        </form>
      </section>

      <section className="card">
        <div className="card-head">{t("profile.password")}</div>
        <form
          className="card-body form"
          onSubmit={(e: FormEvent) => {
            e.preventDefault();
            changePw.mutate();
          }}
        >
          <ErrorAlert message={changePw.isError ? errorText(changePw.error, t) : null} />
          {changePw.isSuccess ? <div className="alert alert-success">{t("profile.changed")}</div> : null}
          <TextField label={t("profile.current")} type="password" value={pw.current} onChange={(e) => setPw({ ...pw, current: e.target.value })} required autoComplete="current-password" />
          <TextField label={t("profile.new")} hint={t("setup.passwordHint")} type="password" value={pw.new} onChange={(e) => setPw({ ...pw, new: e.target.value })} required minLength={8} maxLength={128} autoComplete="new-password" />
          <div>
            <button className="btn" disabled={changePw.isPending}>{t("profile.password")}</button>
          </div>
        </form>
      </section>

      <section className="card">
        <div className="card-head">{t("profile.preferences")}</div>
        <div className="card-body form">
          <div className="field">
            <label htmlFor="pref-theme">{t("shell.theme")}</label>
            <Select id="pref-theme" value={theme} onChange={(v) => setTheme(v as ThemeChoice)}>
              <option value="system">{t("shell.theme.system")}</option>
              <option value="light">{t("shell.theme.light")}</option>
              <option value="dark">{t("shell.theme.dark")}</option>
            </Select>
          </div>
          <div className="field">
            <label htmlFor="pref-lang">{t("shell.language")}</label>
            <Select id="pref-lang" value={lang} onChange={(v) => setLang(v as Lang)}>
              {LANGS.map((l) => (
                <option key={l.code} value={l.code}>{l.label}</option>
              ))}
            </Select>
          </div>
        </div>
      </section>

      <EmailPrefs />

      <ApiTokens />

      <section className="card">
        <div className="card-head">{t("profile.trash")}</div>
        <div className="rows">
          {(trash.data ?? []).length === 0 ? <div className="row muted">{t("profile.trashEmpty")}</div> : null}
          {(trash.data ?? []).map((w) => (
            <div className="row" key={w.id}>
              <div className="row-main">
                <span>{w.name}</span>
                <span className="faint small">/{w.slug} · {t("profile.deletedOn", { date: new Date(w.deleted_at!).toLocaleDateString() })}</span>
              </div>
              <button className="btn btn-sm" onClick={() => restore.mutate(w.slug)} disabled={restore.isPending}>
                {t("profile.restore")}
              </button>
            </div>
          ))}
        </div>
      </section>
    </PlainPage>
  );
}

function EmailPrefs() {
  const { t } = useI18n();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["prefs"], queryFn: () => get<{ email_notifications: boolean; email_enabled: boolean }>("/me/preferences") });
  const set = useMutation({
    mutationFn: (on: boolean) => patch<{ email_notifications: boolean; email_enabled: boolean }>("/me/preferences", { email_notifications: on }),
    onSuccess: (r) => qc.setQueryData(["prefs"], r),
  });
  if (!q.data) return null;
  return (
    <section className="card">
      <label className="row checkbox">
        <div className="row-main">
          <b>{t("prefs.email")}</b>
          <span className="muted small">{q.data.email_enabled ? t("prefs.emailHint") : t("prefs.emailOff")}</span>
        </div>
        <input type="checkbox" className="switch" checked={q.data.email_notifications} disabled={!q.data.email_enabled} onChange={(e) => set.mutate(e.target.checked)} />
      </label>
    </section>
  );
}
