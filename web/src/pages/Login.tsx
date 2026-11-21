import { useMutation } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link, Navigate, useLocation, useNavigate } from "react-router";
import { ApiError, post } from "../api/client";
import type { User } from "../api/types";
import { ErrorAlert, FullPageSpinner, TextField } from "../components/ui";
import { useT } from "../i18n";
import { errorText } from "../lib/errors";
import { useInstance, useMe, useSetMe } from "../lib/session";
import { AuthCard } from "./Setup";

export function LoginPage() {
  const t = useT();
  const instance = useInstance();
  const me = useMe();
  const setMe = useSetMe();
  const navigate = useNavigate();
  const location = useLocation();
  const next = (location.state as { from?: string } | null)?.from ?? "/";
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [adminForm, setAdminForm] = useState(false);

  const login = useMutation({
    mutationFn: () => post<User>("/auth/login", { email, password }),
    onSuccess: (user) => {
      setMe(user);
      navigate(next, { replace: true });
    },
  });

  if (instance.isLoading || me.isLoading) return <FullPageSpinner />;
  if (instance.data?.setup_required) return <Navigate to="/setup" replace />;
  if (me.data) return <Navigate to={next} replace />;

  const info = instance.data as { signup_policy?: string; sso_name?: string | null; password_enabled?: boolean; providers?: { key: string; name: string }[] } | undefined;
  const passwordOn = info?.password_enabled !== false;
  const signupOpen = info?.signup_policy === "open" && passwordOn;
  const providers = info?.providers ?? [];
  const buttons = !!info?.sso_name || providers.length > 0;
  const ssoError = new URLSearchParams(location.search).get("sso_error");
  const error = login.isError
    ? login.error instanceof ApiError && login.error.code === "unauthorized"
      ? t("login.wrong")
      : login.error instanceof ApiError && login.error.code === "rate_limited"
        ? t("login.limited")
        : errorText(login.error, t)
    : null;

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    login.mutate();
  };

  return (
    <AuthCard title={t("login.title")} subtitle={instance.data?.name ? t("login.subtitle", { instance: instance.data.name }) : undefined}>
      {buttons ? (
        <>
          <ErrorAlert message={ssoError} />
          <div className="sso-buttons">
            {providers.map((p) => (
              <a key={p.key} className="btn btn-block" href={`/api/v1/auth/oauth/${p.key}/start?next=${encodeURIComponent(next)}`}>
                {t("login.with", { name: p.name })}
              </a>
            ))}
            {info?.sso_name ? <a className="btn btn-block" href={`/api/v1/auth/oidc/start?next=${encodeURIComponent(next)}`}>{t("login.sso", { name: info.sso_name })}</a> : null}
          </div>
          {passwordOn ? <div className="or-line"><span>{t("login.or")}</span></div> : null}
        </>
      ) : null}
      {!passwordOn && !adminForm ? (
        <button type="button" className="link-btn small" onClick={() => setAdminForm(true)}>
          {t("login.adminPassword")}
        </button>
      ) : null}
      {passwordOn || adminForm ? (
      <form className="form" onSubmit={onSubmit}>
        <ErrorAlert message={error} />
        <TextField label={t("login.email")} type="email" value={email} onChange={(e) => setEmail(e.target.value)} required autoComplete="email" autoFocus />
        <TextField label={t("login.password")} type="password" value={password} onChange={(e) => setPassword(e.target.value)} required autoComplete="current-password" />
        <button className="btn btn-primary btn-block" disabled={login.isPending}>
          {t("login.submit")}
        </button>
        <p className="small" style={{ margin: 0, display: "flex", justifyContent: "space-between" }}>
          <Link to="/forgot">{t("login.forgotLink")}</Link>
          {signupOpen ? (
            <span className="muted">
              {t("login.noAccount")} <Link to="/signup">{t("login.signup")}</Link>
            </span>
          ) : null}
        </p>
      </form>
      ) : null}
    </AuthCard>
  );
}
