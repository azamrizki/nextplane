import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { del, get, patch, post } from "../api/client";
import type { Invite, InviteResult, WsMember, WsRole } from "../api/types";
import { Avatar } from "../components/Avatar";
import { ConfirmDialog, Dialog } from "../components/Dialog";
import { ErrorAlert, FullPageSpinner } from "../components/ui";
import { useT } from "../i18n";
import { copyText } from "../lib/copy";
import { errorText } from "../lib/errors";
import { assignableRoles, can, canRemove, invitableRoles } from "../lib/perm";
import { wq } from "../lib/queries";
import { useMe } from "../lib/session";
import { useWs } from "./WorkspaceHome";

export function MembersPage() {
  const t = useT();
  const ws = useWs();
  const me = useMe().data!;
  const qc = useQueryClient();
  const members = useQuery({ queryKey: wq.members(ws.slug), queryFn: () => get<WsMember[]>(`/workspaces/${ws.slug}/members`) });
  const invites = useQuery({
    queryKey: wq.invites(ws.slug),
    queryFn: () => get<Invite[]>(`/workspaces/${ws.slug}/invites`),
    enabled: can.invite(ws),
  });
  const [inviteOpen, setInviteOpen] = useState(false);
  const [removing, setRemoving] = useState<WsMember | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => {
    qc.invalidateQueries({ queryKey: wq.members(ws.slug) });
    qc.invalidateQueries({ queryKey: wq.invites(ws.slug) });
    qc.invalidateQueries({ queryKey: wq.list });
  };
  const setRole = useMutation({
    mutationFn: (v: { user: string; role: WsRole }) => patch<void>(`/workspaces/${ws.slug}/members/${v.user}`, { role: v.role }),
    onSuccess: refresh,
    onError: (e) => setError(errorText(e, t)),
  });
  const remove = useMutation({
    mutationFn: (user: string) => del<void>(`/workspaces/${ws.slug}/members/${user}`),
    onSuccess: refresh,
    onError: (e) => setError(errorText(e, t)),
  });
  const revoke = useMutation({
    mutationFn: (id: string) => del<void>(`/workspaces/${ws.slug}/invites/${id}`),
    onSuccess: refresh,
  });

  if (members.isLoading) return <FullPageSpinner />;
  return (
    <div className="page">
      <div className="page-head">
        <h1>{t("members.title")}</h1>
        {can.invite(ws) ? (
          <button className="btn btn-primary" onClick={() => setInviteOpen(true)}>
            {t("members.invite")}
          </button>
        ) : null}
      </div>
      <ErrorAlert message={error} />
      <section className="card rows">
        {(members.data ?? []).map((m) => {
          const self = m.user_id === me.id;
          const roles = assignableRoles(ws.role, m.role, self);
          return (
            <div className="row" key={m.user_id}>
              <Avatar name={m.name} email={m.email} src={m.avatar} size={28} />
              <div className="row-main">
                <span className="ellipsis">
                  {m.name || m.email} {self ? <span className="faint">({t("common.you")})</span> : null}
                </span>
                <span className="faint small ellipsis">{m.email}</span>
              </div>
              {roles.length ? (
                <select
                  className="select select-inline"
                  aria-label={t("members.role")}
                  value={m.role}
                  onChange={(e) => {
                    setError(null);
                    setRole.mutate({ user: m.user_id, role: e.target.value as WsRole });
                  }}
                >
                  {[...new Set([m.role, ...roles])].map((r) => (
                    <option key={r} value={r}>
                      {t(`role.${r}`)}
                    </option>
                  ))}
                </select>
              ) : (
                <span className="muted small">{t(`role.${m.role}`)}</span>
              )}
              {!self && canRemove(ws.role, m.role, false) ? (
                <button className="btn btn-sm btn-ghost" onClick={() => setRemoving(m)}>
                  {t("common.remove")}
                </button>
              ) : (
                <span style={{ width: 70 }} />
              )}
            </div>
          );
        })}
      </section>

      {can.invite(ws) && (invites.data?.length ?? 0) > 0 ? (
        <section className="card">
          <div className="card-head">{t("members.pending")}</div>
          <div className="rows">
            {invites.data!.map((i) => (
              <div className="row" key={i.id}>
                <div className="row-main">
                  <span>{i.email}</span>
                  <span className="faint small">
                    {t(`role.${i.role}`)} · {t("members.expires", { date: new Date(i.expires_at).toLocaleDateString() })}
                  </span>
                </div>
                <button className="btn btn-sm" onClick={() => revoke.mutate(i.id)}>
                  {t("members.revoke")}
                </button>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      <InviteDialog open={inviteOpen} onClose={() => setInviteOpen(false)} onDone={refresh} />
      <ConfirmDialog
        open={!!removing}
        onClose={() => setRemoving(null)}
        onConfirm={() => {
          if (removing) remove.mutate(removing.user_id);
          setRemoving(null);
        }}
        title={t("common.remove")}
        body={t("members.removeConfirm", { name: removing?.name || removing?.email || "" })}
        confirmLabel={t("common.remove")}
        danger
      />
    </div>
  );
}

function InviteDialog({ open, onClose, onDone }: { open: boolean; onClose: () => void; onDone: () => void }) {
  const t = useT();
  const ws = useWs();
  const roles = invitableRoles(ws.role);
  const [emails, setEmails] = useState("");
  const [role, setRole] = useState<WsRole>("member");
  const [results, setResults] = useState<InviteResult[] | null>(null);
  const send = useMutation({
    mutationFn: () =>
      post<InviteResult[]>(`/workspaces/${ws.slug}/invites`, {
        invites: emails
          .split(/[\s,;]+/)
          .map((e) => e.trim())
          .filter(Boolean)
          .map((email) => ({ email, role })),
      }),
    onSuccess: (r) => {
      setResults(r);
      onDone();
    },
  });
  const close = () => {
    setEmails("");
    setResults(null);
    send.reset();
    onClose();
  };
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    send.mutate();
  };
  return (
    <Dialog open={open} onClose={close} title={results ? t("members.links") : t("members.invite")} width={520}>
      {results ? (
        <div className="form">
          <p className="muted small" style={{ margin: 0 }}>{t("members.linksHint")}</p>
          {results.map((r) => (
            <div key={r.email} className="field">
              <label>
                {r.email} · {t(`role.${r.role}`)} {r.emailed ? <span className="badge">{t("members.emailed")}</span> : null}
              </label>
              {r.link ? <LinkBox link={r.link} /> : <div className="alert alert-error">{r.error}</div>}
            </div>
          ))}
          <div>
            <button className="btn" onClick={close}>{t("common.close")}</button>
          </div>
        </div>
      ) : (
        <form className="form" onSubmit={onSubmit}>
          <ErrorAlert message={send.isError ? errorText(send.error, t) : null} />
          <div className="field">
            <label htmlFor="invite-emails">{t("members.emails")}</label>
            <textarea id="invite-emails" className="textarea" value={emails} onChange={(e) => setEmails(e.target.value)} required autoFocus placeholder="ana@example.com, bob@example.com" />
            <span className="hint">{t("members.inviteHint")}</span>
          </div>
          <div className="field">
            <label htmlFor="invite-role">{t("members.role")}</label>
            <select id="invite-role" className="select" value={role} onChange={(e) => setRole(e.target.value as WsRole)}>
              {roles.map((r) => (
                <option key={r} value={r}>{t(`role.${r}`)}</option>
              ))}
            </select>
          </div>
          <div>
            <button className="btn btn-primary" disabled={send.isPending || !emails.trim()}>{t("members.send")}</button>
          </div>
        </form>
      )}
    </Dialog>
  );
}

export function LinkBox({ link }: { link: string }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  return (
    <div className="link-box">
      <code title={link}>{link}</code>
      <button
        type="button"
        className="btn btn-sm"
        onClick={async () => {
          setCopied(await copyText(link));
          setTimeout(() => setCopied(false), 1500);
        }}
      >
        {copied ? t("common.copied") : t("common.copy")}
      </button>
    </div>
  );
}
