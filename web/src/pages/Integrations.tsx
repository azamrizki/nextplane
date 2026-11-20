// Workspace integrations for admins: webhooks and GitHub (SPRINT S3.5).
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link, Navigate } from "react-router";
import { del, get, patch, post } from "../api/client";
import { ConfirmDialog } from "../components/Dialog";
import { ErrorAlert, TextField } from "../components/ui";
import { timeAgo } from "../components/WorkItemDetail";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { isWsAdmin } from "../lib/perm";
import { useWs } from "./WorkspaceHome";

const AREAS = ["work_item", "comment", "attachment", "link", "relation", "reaction", "cycle", "module", "view", "page", "intake", "project", "state", "label", "member"] as const;

interface Hook {
  id: string;
  url: string;
  events: string;
  active: boolean;
  created_at: string;
  secret?: string;
}
interface Delivery {
  id: string;
  event: string;
  status: "pending" | "delivered" | "failed";
  attempts: number;
  response_code: number | null;
  error: string | null;
  created_at: string;
  next_attempt_at: string | null;
}

function Copyable({ value, label }: { value: string; label: string }) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  return (
    <div className="row-flex copy-row">
      <input className="input mono small" readOnly value={value} aria-label={label} onFocus={(e) => e.target.select()} />
      <button
        className="btn btn-sm"
        onClick={async () => {
          await navigator.clipboard?.writeText(value).catch(() => {});
          setCopied(true);
        }}
      >
        {copied ? t("in.copied") : t("in.copy")}
      </button>
    </div>
  );
}

function Back() {
  const { t } = useI18n();
  const ws = useWs();
  return <Link to={`/${ws.slug}/settings`} className="small">← {t("ws.settings.title")}</Link>;
}

export function WebhooksPage() {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const base = `/workspaces/${ws.slug}/webhooks`;
  const key = ["webhooks", ws.slug];
  const list = useQuery({ queryKey: key, queryFn: () => get<Hook[]>(base), enabled: isWsAdmin(ws.role) });
  const [url, setUrl] = useState("");
  const [all, setAll] = useState(true);
  const [events, setEvents] = useState<string[]>(["work_item"]);
  const [secret, setSecret] = useState<{ id: string; secret: string } | null>(null);
  const create = useMutation({
    mutationFn: () => post<Hook>(base, { url, events: all ? ["*"] : events }),
    onSuccess: (h) => {
      setSecret({ id: h.id, secret: h.secret ?? "" });
      setUrl("");
      qc.invalidateQueries({ queryKey: key });
    },
  });
  if (!isWsAdmin(ws.role)) return <Navigate to={`/${ws.slug}/settings`} replace />;
  const submit = (e: FormEvent) => {
    e.preventDefault();
    create.mutate();
  };
  return (
    <div className="page">
      <Back />
      <div className="page-head">
        <h1>{t("wh.title")}</h1>
      </div>
      <p className="muted small">{t("wh.intro")}</p>
      {secret ? (
        <div className="alert alert-warning form">
          <span>{t("wh.secretOnce")}</span>
          <Copyable value={secret.secret} label={t("gh.secret")} />
        </div>
      ) : null}
      <section className="card">
        <div className="card-head">{t("wh.add")}</div>
        <form className="card-body form" onSubmit={submit}>
          <ErrorAlert message={create.error ? errorText(create.error, t) : null} />
          <TextField label={t("wh.url")} type="url" value={url} onChange={(e) => setUrl(e.target.value)} required placeholder="https://example.com/hooks/nextplane" />
          <fieldset className="events-box">
            <legend className="small">{t("wh.events")}</legend>
            <label className="checkbox">
              <input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} />
              <span>{t("wh.all")}</span>
            </label>
            {!all ? (
              <div className="events-grid">
                {AREAS.map((a) => (
                  <label key={a} className="checkbox">
                    <input type="checkbox" checked={events.includes(a)} onChange={(e) => setEvents((v) => (e.target.checked ? [...v, a] : v.filter((x) => x !== a)))} />
                    <span>{t(`wh.ev.${a}`)}</span>
                  </label>
                ))}
              </div>
            ) : null}
          </fieldset>
          <div>
            <button className="btn btn-primary" disabled={create.isPending || (!all && !events.length)}>{t("wh.add")}</button>
          </div>
        </form>
      </section>
      {list.isSuccess && !list.data.length ? <p className="faint small">{t("wh.empty")}</p> : null}
      {(list.data ?? []).map((h) => (
        <HookCard key={h.id} hook={h} onSecret={(s) => setSecret({ id: h.id, secret: s })} />
      ))}
    </div>
  );
}

function HookCard({ hook, onSecret }: { hook: Hook; onSecret: (s: string) => void }) {
  const { t, lang } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const base = `/workspaces/${ws.slug}/webhooks/${hook.id}`;
  const [log, setLog] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const refresh = () => qc.invalidateQueries({ queryKey: ["webhooks", ws.slug] });
  const logQ = useQuery({ queryKey: ["webhook-log", hook.id], queryFn: () => get<Delivery[]>(`${base}/deliveries`), enabled: log, refetchInterval: log ? 3000 : false });
  const act = useMutation({
    mutationFn: async (a: "ping" | "rotate" | "toggle" | "delete") => {
      if (a === "ping") return post(`${base}/ping`);
      if (a === "rotate") return post<Hook>(`${base}/rotate`).then((h) => h.secret && onSecret(h.secret));
      if (a === "toggle") return patch(base, { active: !hook.active });
      return del(base);
    },
    onSuccess: (_, a) => {
      refresh();
      if (a === "ping") {
        setLog(true);
        qc.invalidateQueries({ queryKey: ["webhook-log", hook.id] });
      }
    },
  });
  const redeliver = useMutation({ mutationFn: (id: string) => post(`${base}/deliveries/${id}/redeliver`), onSuccess: () => qc.invalidateQueries({ queryKey: ["webhook-log", hook.id] }) });
  const events = hook.events === "*" ? t("wh.all") : hook.events.split(",").map((e) => t(`wh.ev.${e as "work_item"}`)).join(", ");
  return (
    <section className="card hook-card">
      <div className="card-body form">
        <div className="row-flex">
          <span className="mono small ellipsis">{hook.url}</span>
          <label className="checkbox">
            <input type="checkbox" checked={hook.active} onChange={() => act.mutate("toggle")} />
            <span>{t("wh.active")}</span>
          </label>
        </div>
        <span className="faint small">{t("wh.events")}: {events}</span>
        <ErrorAlert message={act.error ? errorText(act.error, t) : null} />
        <div className="page-actions">
          <button className="btn btn-sm" onClick={() => setLog((v) => !v)}>{log ? t("wh.hideLog") : t("wh.log")}</button>
          <button className="btn btn-sm" onClick={() => act.mutate("ping")}>{t("wh.ping")}</button>
          <button className="btn btn-sm" onClick={() => act.mutate("rotate")}>{t("wh.rotate")}</button>
          <button className="btn btn-sm btn-ghost danger-text" onClick={() => setDeleting(true)}>{t("wh.delete")}</button>
        </div>
        {log ? (
          logQ.data?.length ? (
            <table className="data-table">
              <tbody>
                {logQ.data.map((d) => (
                  <tr key={d.id}>
                    <td className="faint">{timeAgo(d.created_at, t, lang)}</td>
                    <td className="mono">{d.event}</td>
                    <td>
                      <span className={`hook-st st-${d.status}`}>{t(`wh.st.${d.status}`)}</span>
                    </td>
                    <td className="faint">{d.response_code ?? d.error ?? ""}</td>
                    <td className="faint">{t("wh.attempts", { n: d.attempts })}</td>
                    <td>{d.status !== "pending" ? <button className="btn btn-sm btn-ghost" onClick={() => redeliver.mutate(d.id)}>{t("wh.redeliver")}</button> : null}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <p className="faint small">{t("wh.noLog")}</p>
          )
        ) : null}
      </div>
      <ConfirmDialog open={deleting} onClose={() => setDeleting(false)} onConfirm={() => act.mutate("delete")} title={t("wh.delete")} body={t("wh.deleteBody")} confirmLabel={t("wh.delete")} danger />
    </section>
  );
}

interface Github {
  connected: boolean;
  path: string | null;
  secret?: string;
  close_on_merge: boolean;
  last_event_at: string | null;
}

export function GithubPage() {
  const { t, lang } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const base = `/workspaces/${ws.slug}/github`;
  const key = ["github", ws.slug];
  const q = useQuery({ queryKey: key, queryFn: () => get<Github>(base), enabled: isWsAdmin(ws.role) });
  const [secret, setSecret] = useState("");
  const connect = useMutation({
    mutationFn: () => post<Github>(base),
    onSuccess: (g) => {
      setSecret(g.secret ?? "");
      qc.setQueryData(key, g);
    },
  });
  const toggle = useMutation({ mutationFn: (on: boolean) => patch(base, { close_on_merge: on }), onSuccess: () => qc.invalidateQueries({ queryKey: key }) });
  const off = useMutation({
    mutationFn: () => del(base),
    onSuccess: () => {
      setSecret("");
      qc.invalidateQueries({ queryKey: key });
    },
  });
  if (!isWsAdmin(ws.role)) return <Navigate to={`/${ws.slug}/settings`} replace />;
  const g = q.data;
  const err = connect.error ?? toggle.error ?? off.error;
  return (
    <div className="page">
      <Back />
      <div className="page-head">
        <h1>{t("gh.title")}</h1>
      </div>
      <p className="muted small">{t("gh.intro")}</p>
      <ErrorAlert message={err ? errorText(err, t) : null} />
      {g && !g.connected ? (
        <div>
          <button className="btn btn-primary" onClick={() => connect.mutate()} disabled={connect.isPending}>{t("gh.connect")}</button>
        </div>
      ) : null}
      {g?.connected && g.path ? (
        <section className="card">
          <div className="card-body form">
            <p className="small">{t("gh.steps")}</p>
            <div className="field">
              <label>{t("gh.payload")}</label>
              <Copyable value={`${location.origin}${g.path}`} label={t("gh.payload")} />
            </div>
            <div className="field">
              <label>{t("gh.secret")}</label>
              {secret ? <Copyable value={secret} label={t("gh.secret")} /> : <span className="faint small">{t("gh.secretHidden")}</span>}
            </div>
            <label className="checkbox">
              <input type="checkbox" checked={g.close_on_merge} onChange={(e) => toggle.mutate(e.target.checked)} />
              <span>{t("gh.close")}</span>
            </label>
            <span className="faint small">{g.last_event_at ? t("gh.last", { when: timeAgo(g.last_event_at, t, lang) }) : t("gh.never")}</span>
            <div className="page-actions">
              <button className="btn btn-sm" onClick={() => connect.mutate()}>{t("gh.renew")}</button>
              <button className="btn btn-sm btn-ghost danger-text" onClick={() => off.mutate()}>{t("gh.disconnect")}</button>
            </div>
          </div>
        </section>
      ) : null}
    </div>
  );
}
