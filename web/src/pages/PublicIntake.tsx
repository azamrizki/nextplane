// The public intake form at /intake/<token>: no account needed (SPRINT S3.3).
import { useMutation, useQuery } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { useParams } from "react-router";
import { get, post } from "../api/client";
import { ErrorAlert, FullPageSpinner, TextField } from "../components/ui";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { AuthCard } from "./Setup";

export function PublicIntakePage() {
  const { t } = useI18n();
  const { token = "" } = useParams();
  const info = useQuery({ queryKey: ["public-intake", token], queryFn: () => get<{ workspace: string; project: string }>(`/public/intake/${token}`) });
  const blank = { name: "", email: "", title: "", description: "", website: "" };
  const [f, setF] = useState(blank);
  const send = useMutation({ mutationFn: () => post(`/public/intake/${token}`, f) });
  if (info.isLoading) return <FullPageSpinner />;
  if (!info.data) return <AuthCard title={t("pf.gone")}>{null}</AuthCard>;
  if (send.isSuccess) {
    return (
      <AuthCard title={t("pf.thanks")}>
        <button
          className="btn btn-block"
          onClick={() => {
            setF({ ...blank, name: f.name, email: f.email });
            send.reset();
          }}
        >
          {t("pf.another")}
        </button>
      </AuthCard>
    );
  }
  const submit = (e: FormEvent) => {
    e.preventDefault();
    send.mutate();
  };
  return (
    <AuthCard title={t("pf.title", { project: info.data.project })} subtitle={t("pf.subtitle", { workspace: info.data.workspace })}>
      <form className="form" onSubmit={submit}>
        <ErrorAlert message={send.error ? errorText(send.error, t) : null} />
        <TextField label={t("pf.what")} value={f.title} onChange={(e) => setF({ ...f, title: e.target.value })} required maxLength={255} autoFocus />
        <div className="field">
          <label htmlFor="pf-details">{t("pf.details")}</label>
          <textarea id="pf-details" className="textarea" rows={6} maxLength={20000} value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} />
        </div>
        <TextField label={t("pf.name")} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} maxLength={120} autoComplete="name" />
        <TextField label={t("pf.email")} type="email" value={f.email} onChange={(e) => setF({ ...f, email: e.target.value })} autoComplete="email" />
        {/* Hidden from people; bots fill it in. */}
        <input className="hp" tabIndex={-1} autoComplete="off" aria-hidden="true" name="website" value={f.website} onChange={(e) => setF({ ...f, website: e.target.value })} />
        <button className="btn btn-primary btn-block" disabled={send.isPending}>{t("pf.send")}</button>
      </form>
    </AuthCard>
  );
}
