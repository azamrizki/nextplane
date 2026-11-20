import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link, Navigate, useNavigate, useSearchParams } from "react-router";
import { post } from "../api/client";
import type { User } from "../api/types";
import { ErrorAlert, FullPageSpinner, Logo, TextField } from "../components/ui";
import { useT } from "../i18n";
import { errorText } from "../lib/errors";
import { qk, useInstance } from "../lib/session";

export function SetupPage() {
  const t = useT();
  const [params] = useSearchParams();
  const token = params.get("token") ?? "";
  const instance = useInstance();
  const verify = useQuery({
    queryKey: ["setup-verify", token],
    queryFn: () => post<{ ok: boolean }>("/setup/verify", { token }),
    enabled: !!token && instance.data?.setup_required === true,
    retry: false,
  });

  if (instance.isLoading) return <FullPageSpinner />;
  if (instance.data && !instance.data.setup_required) {
    return (
      <AuthCard title={t("setup.done")}>
        <Link to="/login" className="btn btn-primary btn-block">
          {t("setup.goLogin")}
        </Link>
      </AuthCard>
    );
  }
  if (!token || verify.isError) {
    return (
      <AuthCard title={t("setup.invalidTitle")}>
        <p className="muted">{t("setup.invalidBody")}</p>
      </AuthCard>
    );
  }
  if (!verify.isSuccess) {
    return (
      <AuthCard title={t("setup.title")}>
        <p className="muted">{t("setup.checking")}</p>
      </AuthCard>
    );
  }
  return <SetupForm token={token} />;
}

function SetupForm({ token }: { token: string }) {
  const t = useT();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [form, setForm] = useState({ instance_name: "", name: "", email: "", password: "" });
  const set = (k: keyof typeof form) => (e: { target: { value: string } }) =>
    setForm((f) => ({ ...f, [k]: e.target.value }));

  const submit = useMutation({
    mutationFn: () => post<User>("/setup", { token, ...form }),
    onSuccess: async (user) => {
      qc.setQueryData(qk.me, user);
      await qc.invalidateQueries({ queryKey: qk.instance });
      navigate("/", { replace: true });
    },
  });

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    submit.mutate();
  };

  return (
    <AuthCard title={t("setup.title")} subtitle={t("setup.subtitle")}>
      <form className="form" onSubmit={onSubmit}>
        <ErrorAlert message={submit.isError ? errorText(submit.error, t) : null} />
        <TextField label={t("setup.instanceName")} hint={t("setup.instanceNameHint")} value={form.instance_name} onChange={set("instance_name")} required maxLength={80} autoFocus />
        <TextField label={t("setup.name")} value={form.name} onChange={set("name")} maxLength={80} autoComplete="name" />
        <TextField label={t("setup.email")} type="email" value={form.email} onChange={set("email")} required autoComplete="email" />
        <TextField label={t("setup.password")} hint={t("setup.passwordHint")} type="password" value={form.password} onChange={set("password")} required minLength={8} maxLength={128} autoComplete="new-password" />
        <button className="btn btn-primary btn-block" disabled={submit.isPending}>
          {t("setup.submit")}
        </button>
      </form>
    </AuthCard>
  );
}

export function AuthCard({ title, subtitle, children }: { title: string; subtitle?: string; children: React.ReactNode }) {
  return (
    <div className="auth-page">
      <div className="auth-card">
        <div className="auth-head">
          <Logo />
          <h1>{title}</h1>
          {subtitle ? <p className="muted small" style={{ margin: 0 }}>{subtitle}</p> : null}
        </div>
        {children}
      </div>
    </div>
  );
}

export function RedirectToSetup() {
  return <Navigate to="/setup" replace />;
}
