import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { get, post } from "../api/client";
import type { InviteInfo, User } from "../api/types";
import { ErrorAlert, FullPageSpinner, TextField } from "../components/ui";
import { useT } from "../i18n";
import { errorText } from "../lib/errors";
import { wq } from "../lib/queries";
import { qk, useMe } from "../lib/session";
import { AuthCard } from "./Setup";
import { useInstanceFull } from "./Account";

export function InvitePage() {
  const t = useT();
  const { token = "" } = useParams();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const me = useMe();
  const instance = useInstanceFull();
  const info = useQuery({ queryKey: ["invite", token], queryFn: () => get<InviteInfo>(`/invites/${token}`), retry: false });
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");

  const done = async (slug: string) => {
    await qc.invalidateQueries({ queryKey: wq.list });
    navigate(`/${slug}`, { replace: true });
  };
  const accept = useMutation({
    mutationFn: () => post<{ workspace_slug: string }>(`/invites/${token}/accept`),
    onSuccess: (r) => done(r.workspace_slug),
  });
  const signup = useMutation({
    mutationFn: () => post<{ user: User; workspace_slug: string }>(`/invites/${token}/signup`, { name, password }),
    onSuccess: (r) => {
      qc.setQueryData(qk.me, r.user);
      done(r.workspace_slug);
    },
  });

  if (info.isLoading || me.isLoading) return <FullPageSpinner />;
  if (info.isError || !info.data) {
    return (
      <AuthCard title={t("invite.invalid")}>
        <Link to="/" className="btn btn-block">{t("common.back")}</Link>
      </AuthCard>
    );
  }
  const i = info.data;
  const title = t("invite.title", { workspace: i.workspace_name });
  const body = t("invite.body", { inviter: i.invited_by_name, email: i.email, role: t(`role.${i.role}`) });

  if (me.data) {
    const right = me.data.email === i.email;
    return (
      <AuthCard title={title} subtitle={body}>
        <ErrorAlert message={accept.isError ? errorText(accept.error, t) : null} />
        {right ? (
          <button className="btn btn-primary btn-block" onClick={() => accept.mutate()} disabled={accept.isPending}>
            {t("invite.accept")}
          </button>
        ) : (
          <div className="alert alert-warning">{t("invite.wrongAccount", { me: me.data.email, email: i.email })}</div>
        )}
      </AuthCard>
    );
  }
  const sso = (instance.data as { sso_name?: string | null } | undefined)?.sso_name;
  const ssoButton = sso ? (
    <a className="btn btn-block" href={`/api/v1/auth/oidc/start?next=${encodeURIComponent(`/invite/${token}`)}`}>{t("login.sso", { name: sso })}</a>
  ) : null;
  if (i.account_exists) {
    return (
      <AuthCard title={title} subtitle={body}>
        <Link to="/login" state={{ from: `/invite/${token}` }} className="btn btn-primary btn-block">
          {t("invite.loginToAccept")}
        </Link>
        {ssoButton}
      </AuthCard>
    );
  }
  if (!i.signup_allowed) {
    return (
      <AuthCard title={title} subtitle={body}>
        <div className="alert alert-warning">{t("invite.signupClosed")}</div>
      </AuthCard>
    );
  }
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    signup.mutate();
  };
  return (
    <AuthCard title={title} subtitle={body}>
      {ssoButton ? <>{ssoButton}<div className="or-line"><span>{t("login.or")}</span></div></> : null}
      <form className="form" onSubmit={onSubmit}>
        <ErrorAlert message={signup.isError ? errorText(signup.error, t) : null} />
        <TextField label={t("setup.email")} value={i.email} disabled />
        <TextField label={t("setup.name")} value={name} onChange={(e) => setName(e.target.value)} maxLength={80} autoComplete="name" autoFocus />
        <TextField label={t("setup.password")} hint={t("setup.passwordHint")} type="password" value={password} onChange={(e) => setPassword(e.target.value)} required minLength={8} maxLength={128} autoComplete="new-password" />
        <button className="btn btn-primary btn-block" disabled={signup.isPending}>{t("invite.createAccount")}</button>
      </form>
    </AuthCard>
  );
}
