import { KeyRound } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { del, get, post } from "../api/client";
import type { MyWorkspace, User } from "../api/types";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { LinkBox } from "../pages/Members";
import { Avatar } from "./Avatar";
import { Dialog } from "./Dialog";
import { ErrorAlert, TextField } from "./ui";
import { NumberField } from "./NumberField";
import { Select } from "./Select";

interface Token {
  id: string;
  name: string;
  scope: "read" | "write";
  expires_at: string | null;
  last_used_at: string | null;
  created_at: string;
}

function TokenRow({ tk, onRevoke }: { tk: Token; onRevoke?: () => void }) {
  const { t, lang } = useI18n();
  const d = (s: string) => new Date(s).toLocaleDateString(lang === "id" ? "id-ID" : "en-US");
  return (
    <div className="row">
      <KeyRound aria-hidden className="inline-icon" />
      <div className="row-main">
        <span>{tk.name}</span>
        <span className="faint small">
          {tk.scope === "read" ? t("tok.read") : t("tok.write")} · {tk.last_used_at ? t("tok.lastUsed", { date: d(tk.last_used_at) }) : t("tok.never")}
          {tk.expires_at ? ` · ${d(tk.expires_at)}` : ""}
        </span>
      </div>
      {onRevoke ? <button className="btn btn-sm" onClick={onRevoke}>{t("tok.revoke")}</button> : null}
    </div>
  );
}

export function ApiTokens() {
  const { t } = useI18n();
  const qc = useQueryClient();
  const list = useQuery({ queryKey: ["tokens"], queryFn: () => get<Token[]>("/me/tokens") });
  const [open, setOpen] = useState(false);
  const [f, setF] = useState({ name: "", scope: "write", days: "" });
  const [secret, setSecret] = useState<string | null>(null);
  const create = useMutation({
    mutationFn: () => post<{ token: string }>("/me/tokens", { name: f.name, scope: f.scope, expires_in_days: f.days ? Number(f.days) : undefined }),
    onSuccess: (r) => {
      setSecret(r.token);
      setF({ name: "", scope: "write", days: "" });
      qc.invalidateQueries({ queryKey: ["tokens"] });
    },
  });
  const revoke = useMutation({ mutationFn: (id: string) => del<void>(`/me/tokens/${id}`), onSuccess: () => qc.invalidateQueries({ queryKey: ["tokens"] }) });
  return (
    <section className="card">
      <div className="card-head row-flex">
        {t("tok.title")}
        <button className="btn btn-sm" onClick={() => { setSecret(null); setOpen(true); }}>{t("tok.new")}</button>
      </div>
      <p className="muted small" style={{ margin: "var(--space-3) var(--space-4) 0" }}>{t("tok.hint")}</p>
      <div className="rows">
        {(list.data ?? []).length === 0 ? <div className="row muted">{t("tok.none")}</div> : null}
        {(list.data ?? []).map((tk) => <TokenRow key={tk.id} tk={tk} onRevoke={() => revoke.mutate(tk.id)} />)}
      </div>
      <Dialog open={open} onClose={() => setOpen(false)} title={t("tok.new")} width={520}>
        {secret ? (
          <div className="form">
            <div className="alert alert-warning">{t("tok.created")}</div>
            <LinkBox link={secret} />
          </div>
        ) : (
          <form className="form" onSubmit={(e) => { e.preventDefault(); create.mutate(); }}>
            <ErrorAlert message={create.isError ? errorText(create.error, t) : null} />
            <TextField label={t("tok.name")} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} required maxLength={80} autoFocus />
            <div className="field">
              <label htmlFor="tok-scope">{t("tok.scope")}</label>
              <Select id="tok-scope" value={f.scope} onChange={(v) => setF({ ...f, scope: v })}>
                <option value="write">{t("tok.write")}</option>
                <option value="read">{t("tok.read")}</option>
              </Select>
            </div>
            <div className="field">
              <label htmlFor="tok-days">{t("tok.expiry")}</label>
              <NumberField id="tok-days" min={1} max={3650} value={f.days} onChange={(v) => setF({ ...f, days: v })} />
            </div>
            <div><button className="btn btn-primary" disabled={create.isPending}>{t("tok.new")}</button></div>
          </form>
        )}
      </Dialog>
    </section>
  );
}

interface Bot extends User {
  tokens: Token[];
}

export function Bots({ ws }: { ws: MyWorkspace }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const key = ["bots", ws.slug];
  const list = useQuery({ queryKey: key, queryFn: () => get<Bot[]>(`/workspaces/${ws.slug}/bots`) });
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [secret, setSecret] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = () => {
    qc.invalidateQueries({ queryKey: key });
    qc.invalidateQueries({ queryKey: ["workspace", ws.slug, "members"] });
  };
  const create = useMutation({
    mutationFn: () => post<{ token: string }>(`/workspaces/${ws.slug}/bots`, { name }),
    onSuccess: (r) => {
      setSecret(r.token);
      setName("");
      refresh();
    },
  });
  const act = async (fn: () => Promise<{ token?: string } | void>) => {
    try {
      const r = await fn();
      if (r && "token" in r && r.token) {
        setSecret(r.token);
        setOpen(true);
      }
      refresh();
    } catch (e) {
      setError(errorText(e, t));
    }
  };
  return (
    <section className="card">
      <div className="card-head row-flex">
        {t("bots.title")}
        <button className="btn btn-sm" onClick={() => { setSecret(null); setOpen(true); }}>{t("bots.new")}</button>
      </div>
      <p className="muted small" style={{ margin: "var(--space-3) var(--space-4) 0" }}>{t("bots.hint")}</p>
      <ErrorAlert message={error} />
      <div className="rows">
        {(list.data ?? []).length === 0 ? <div className="row muted">{t("bots.none")}</div> : null}
        {(list.data ?? []).map((b) => (
          <div className="row" key={b.id}>
            <Avatar name={b.name} email={b.email} size={28} />
            <div className="row-main">
              <span>{b.name} <span className="badge">bot</span></span>
              <span className="faint small">{b.tokens[0]?.last_used_at ? t("tok.lastUsed", { date: new Date(b.tokens[0].last_used_at).toLocaleDateString() }) : t("tok.never")}</span>
            </div>
            <button className="btn btn-sm" onClick={() => act(() => post<{ token: string }>(`/workspaces/${ws.slug}/bots/${b.id}/token`))}>{t("bots.rotate")}</button>
            <button className="btn btn-sm btn-ghost" onClick={() => act(() => del<void>(`/workspaces/${ws.slug}/bots/${b.id}`))}>{t("bots.remove")}</button>
          </div>
        ))}
      </div>
      <Dialog open={open} onClose={() => setOpen(false)} title={t("bots.new")} width={520}>
        {secret ? (
          <div className="form">
            <div className="alert alert-warning">{t("bots.created")}</div>
            <LinkBox link={secret} />
          </div>
        ) : (
          <form className="form" onSubmit={(e) => { e.preventDefault(); create.mutate(); }}>
            <ErrorAlert message={create.isError ? errorText(create.error, t) : null} />
            <TextField label={t("bots.name")} value={name} onChange={(e) => setName(e.target.value)} required maxLength={80} autoFocus placeholder="NextBoard agent" />
            <div><button className="btn btn-primary" disabled={create.isPending}>{t("bots.new")}</button></div>
          </form>
        )}
      </Dialog>
    </section>
  );
}
