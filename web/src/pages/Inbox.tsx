import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";
import { get, post } from "../api/client";
import type { Notification, WsMember } from "../api/types";
import { Avatar } from "../components/Avatar";
import { Popover } from "../components/Layout";
import { fmtDate } from "../components/Props";
import { timeAgo } from "../components/WorkItemDetail";
import { useI18n } from "../i18n";
import { wq } from "../lib/queries";
import { useWs } from "./WorkspaceHome";

type Filter = "unread" | "all" | "snoozed" | "archived";

export const nq = {
  list: (slug: string, f: string) => ["notifications", slug, "list", f] as const,
  count: (slug: string) => ["notifications", slug, "count"] as const,
};

/** Unread count for the sidebar badge; refreshed by realtime events and once a minute. */
export function useUnreadCount(slug: string) {
  return useQuery({
    queryKey: nq.count(slug),
    queryFn: () => get<{ unread: number }>(`/workspaces/${slug}/notifications/count`).then((r) => r.unread),
    refetchInterval: 60_000,
  });
}

const iso = (d: Date) => d.toISOString().slice(0, 10);

export function InboxPage() {
  const { t, lang } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [filter, setFilter] = useState<Filter>("unread");
  const list = useQuery({ queryKey: nq.list(ws.slug, filter), queryFn: () => get<Notification[]>(`/workspaces/${ws.slug}/notifications?filter=${filter}`) });
  const members = useQuery({ queryKey: wq.members(ws.slug), queryFn: () => get<WsMember[]>(`/workspaces/${ws.slug}/members`), enabled: ws.role !== "guest" });
  const refresh = () => qc.invalidateQueries({ queryKey: ["notifications", ws.slug] });
  const mark = useMutation({
    mutationFn: (v: { id: string; action: string; until?: string }) => post<void>(`/workspaces/${ws.slug}/notifications/${v.id}/${v.action}`, v.until ? { until: v.until } : {}),
    onSuccess: refresh,
  });
  const readAll = useMutation({ mutationFn: () => post(`/workspaces/${ws.slug}/notifications/read-all`), onSuccess: refresh });
  const who = (id: string) => members.data?.find((m) => m.user_id === id);
  const line = (n: Notification) => {
    const m = who(n.actor_id);
    const name = m ? m.name || m.email : "—";
    if (n.kind !== "updated") return t(`n.${n.kind}`, { who: name });
    if (n.field === "intake") {
      if (n.new_value === "pending" && !n.old_value) return t("n.intakeNew", { who: m ? name : t("in.someone") });
      return t("n.intakeDone", { who: name, status: t(`in.st.${(n.new_value ?? "pending") as "pending"}`) });
    }
    const field = n.field ? t(`field.${n.field}` as "field.title") : "";
    if (n.field === "priority" && n.new_value) return t("n.updatedTo", { who: name, field, value: t(`prio.${n.new_value as "none"}`) });
    if ((n.field === "due_date" || n.field === "start_date") && n.new_value) return t("n.updatedTo", { who: name, field, value: fmtDate(n.new_value, lang) });
    return t("n.updated", { who: name, field });
  };
  const open = (n: Notification) => {
    if (!n.read_at) mark.mutate({ id: n.id, action: "read" });
    // Requests still in intake open there; accepted ones are ordinary work items.
    const inIntake = n.field === "intake" && n.new_value !== "accepted";
    navigate(`/${ws.slug}/projects/${n.project_id}/${inIntake ? "intake" : "work-items"}/${n.work_item_id}`);
  };
  const tomorrow = new Date(Date.now() + 86_400_000);
  const nextWeek = new Date(Date.now() + 7 * 86_400_000);
  const rows = list.data ?? [];
  return (
    <div className="page page-wide">
      <div className="page-head">
        <h1>{t("nav.inbox")}</h1>
        <div className="seg" role="tablist">
          {(["unread", "all", "snoozed", "archived"] as Filter[]).map((f) => (
            <button key={f} role="tab" aria-selected={filter === f} className={filter === f ? "on" : ""} onClick={() => setFilter(f)}>{t(`inbox.${f}`)}</button>
          ))}
        </div>
        {filter === "unread" && rows.length ? <button className="btn btn-sm" onClick={() => readAll.mutate()}>{t("inbox.readAll")}</button> : null}
      </div>
      {list.isSuccess && rows.length === 0 ? <div className="card empty">{t("inbox.empty")}</div> : null}
      {rows.length ? (
        <ol className="rows card inbox" aria-label={t("nav.inbox")}>
          {rows.map((n) => {
            const m = who(n.actor_id);
            return (
              <li key={n.id} className={"row inbox-row" + (n.read_at ? "" : " unread")}>
                {n.read_at ? <span className="unread-dot off" aria-hidden /> : <span className="unread-dot" aria-label={t("inbox.unread")} />}
                <Avatar name={m?.name ?? ""} email={m?.email} src={m?.avatar} size={28} />
                <button className="row-main inbox-open" onClick={() => open(n)}>
                  <span className="ellipsis"><span className="mono faint small">{n.identifier}-{n.sequence}</span> {n.title}</span>
                  <span className="muted small">{line(n)}</span>
                </button>
                <span className="faint small" title={new Date(n.created_at).toLocaleString()}>{timeAgo(n.created_at, t, lang)}</span>
                <span className="inbox-actions">
                  {filter !== "archived" ? (
                    <button className="btn btn-sm btn-ghost" onClick={() => mark.mutate({ id: n.id, action: n.read_at ? "unread" : "read" })}>{n.read_at ? t("inbox.markUnread") : t("inbox.markRead")}</button>
                  ) : null}
                  {filter === "snoozed" ? (
                    <button className="btn btn-sm btn-ghost" onClick={() => mark.mutate({ id: n.id, action: "unsnooze" })}>{t("inbox.unsnooze")}</button>
                  ) : filter !== "archived" ? (
                    <SnoozeMenu onPick={(until) => mark.mutate({ id: n.id, action: "snooze", until })} options={[[t("inbox.tomorrow"), iso(tomorrow)], [t("inbox.nextWeek"), iso(nextWeek)]]} />
                  ) : null}
                  <button className="btn btn-sm btn-ghost" onClick={() => mark.mutate({ id: n.id, action: n.archived_at ? "unarchive" : "archive" })}>{n.archived_at ? t("inbox.unarchive") : t("inbox.archive")}</button>
                </span>
              </li>
            );
          })}
        </ol>
      ) : null}
    </div>
  );
}

function SnoozeMenu({ onPick, options }: { onPick: (until: string) => void; options: [string, string][] }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  return (
    <Popover open={open} onClose={() => setOpen(false)} trigger={<button className="btn btn-sm btn-ghost" onClick={() => setOpen((o) => !o)} aria-expanded={open}>{t("inbox.snooze")}</button>}>
      <div className="menu popover-right" role="menu" style={{ position: "absolute", right: 0 }}>
        {options.map(([label, until]) => (
          <button key={until} className="menu-item" role="menuitem" onClick={() => { setOpen(false); onPick(until); }}>{label}</button>
        ))}
      </div>
    </Popover>
  );
}
