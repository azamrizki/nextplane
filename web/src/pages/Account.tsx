import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link, Navigate, useNavigate, useParams } from "react-router";
import { get, post } from "../api/client";
import type { InstanceInfoFull, User } from "../api/types";
import { ErrorAlert, FullPageSpinner, TextField } from "../components/ui";
import { useT } from "../i18n";
import { errorText } from "../lib/errors";
import { qk, useMe } from "../lib/session";
import { AuthCard } from "./Setup";

export function useInstanceFull() {
  return useQuery({ queryKey: qk.instance, queryFn: () => get<InstanceInfoFull>("/instance") });
}

export function SignupPage() {
  const t = useT();
  const instance = useInstanceFull();
  const me = useMe();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [f, setF] = useState({ email: "", name: "", password: "" });
  const signup = useMutation({
    mutationFn: () => post<User>("/auth/signup", f),
    onSuccess: (u) => {
      qc.setQueryData(qk.me, u);
      navigate("/", { replace: true });
    },
  });
  if (instance.isLoading || me.isLoading) return <FullPageSpinner />;
  if (me.data) return <Navigate to="/" replace />;
  if (instance.data?.signup_policy !== "open") return <Navigate to="/login" replace />;
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    signup.mutate();
  };
  return (
    <AuthCard title={t("signup.title")} subtitle={instance.data.name}>
      <form className="form" onSubmit={onSubmit}>
        <ErrorAlert message={signup.isError ? errorText(signup.error, t) : null} />
        <TextField label={t("setup.name")} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} maxLength={80} autoComplete="name" autoFocus />
        <TextField label={t("setup.email")} type="email" value={f.email} onChange={(e) => setF({ ...f, email: e.target.value })} required autoComplete="email" />
        <TextField label={t("setup.password")} hint={t("setup.passwordHint")} type="password" value={f.password} onChange={(e) => setF({ ...f, password: e.target.value })} required minLength={8} maxLength={128} autoComplete="new-password" />
        <button className="btn btn-primary btn-block" disabled={signup.isPending}>{t("signup.submit")}</button>
        <p className="small muted" style={{ margin: 0 }}>
          {t("signup.haveAccount")} <Link to="/login">{t("login.submit")}</Link>
        </p>
      </form>
    </AuthCard>
  );
}

export function ForgotPage() {
  const t = useT();
  const instance = useInstanceFull();
  const [email, setEmail] = useState("");
  const forgot = useMutation({ mutationFn: () => post<void>("/auth/forgot", { email }) });
  if (instance.isLoading) return <FullPageSpinner />;
  if (!instance.data?.email_enabled) {
    return (
      <AuthCard title={t("forgot.title")}>
        <p className="muted" style={{ margin: 0 }}>{t("forgot.noEmail")}</p>
        <Link to="/login" className="small">← {t("common.back")}</Link>
      </AuthCard>
    );
  }
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    forgot.mutate();
  };
  return (
    <AuthCard title={t("forgot.title")} subtitle={t("forgot.body")}>
      {forgot.isSuccess ? (
        <div className="alert alert-success">{t("forgot.sent")}</div>
      ) : (
        <form className="form" onSubmit={onSubmit}>
          <ErrorAlert message={forgot.isError ? errorText(forgot.error, t) : null} />
          <TextField label={t("setup.email")} type="email" value={email} onChange={(e) => setEmail(e.target.value)} required autoComplete="email" autoFocus />
          <button className="btn btn-primary btn-block" disabled={forgot.isPending}>{t("forgot.submit")}</button>
        </form>
      )}
      <Link to="/login" className="small">← {t("common.back")}</Link>
    </AuthCard>
  );
}

export function ResetPage() {
  const t = useT();
  const { token = "" } = useParams();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const info = useQuery({ queryKey: ["reset", token], queryFn: () => get<{ email: string }>(`/auth/reset/${token}`), retry: false });
  const [password, setPassword] = useState("");
  const reset = useMutation({
    mutationFn: () => post<User>(`/auth/reset/${token}`, { password }),
    onSuccess: (u) => {
      qc.setQueryData(qk.me, u);
      navigate("/", { replace: true });
    },
  });
  if (info.isLoading) return <FullPageSpinner />;
  if (info.isError) {
    return (
      <AuthCard title={t("reset.invalid")}>
        <Link to="/login" className="btn btn-block">{t("common.back")}</Link>
      </AuthCard>
    );
  }
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    reset.mutate();
  };
  return (
    <AuthCard title={t("reset.title")} subtitle={t("reset.for", { email: info.data!.email })}>
      <form className="form" onSubmit={onSubmit}>
        <ErrorAlert message={reset.isError ? errorText(reset.error, t) : null} />
        <TextField label={t("profile.new")} hint={t("setup.passwordHint")} type="password" value={password} onChange={(e) => setPassword(e.target.value)} required minLength={8} maxLength={128} autoComplete="new-password" autoFocus />
        <button className="btn btn-primary btn-block" disabled={reset.isPending}>{t("reset.submit")}</button>
      </form>
    </AuthCard>
  );
}
