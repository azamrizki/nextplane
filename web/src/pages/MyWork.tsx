import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { get } from "../api/client";
import type { WorkItem } from "../api/types";
import { PriorityIcon, fmtDate, isOverdue } from "../components/Props";
import { useI18n } from "../i18n";
import { useWs } from "./WorkspaceHome";

type Kind = "assigned" | "created" | "subscribed";
interface Row extends WorkItem {
  identifier: string;
  project_name: string;
}

export function MyWorkPage() {
  const { t, lang } = useI18n();
  const ws = useWs();
  const [kind, setKind] = useState<Kind>("assigned");
  const [closed, setClosed] = useState(false);
  const q = useQuery({
    queryKey: ["my-work", ws.slug, kind, closed],
    queryFn: () => get<Row[]>(`/workspaces/${ws.slug}/my-work?kind=${kind}${closed ? "&include_closed=true" : ""}`),
  });
  const rows = q.data ?? [];
  return (
    <div className="page page-wide">
      <div className="page-head">
        <h1>{t("nav.myWork")}</h1>
        <label className="checkbox small"><input type="checkbox" checked={closed} onChange={(e) => setClosed(e.target.checked)} />{t("mw.closed")}</label>
        <div className="seg" role="tablist">
          {(["assigned", "created", "subscribed"] as Kind[]).map((k) => (
            <button key={k} role="tab" aria-selected={kind === k} className={kind === k ? "on" : ""} onClick={() => setKind(k)}>{t(`mw.${k}`)}</button>
          ))}
        </div>
      </div>
      {q.isSuccess && rows.length === 0 ? <div className="card empty">{t("mw.empty")}</div> : null}
      {rows.length ? (
        <div className="rows card">
          {rows.map((w) => (
            <Link key={w.id} to={`/${ws.slug}/projects/${w.project_id}/work-items/${w.id}`} className="row row-link">
              <PriorityIcon p={w.priority} />
              <span className="mono faint small" style={{ width: 70 }}>{w.identifier}-{w.sequence}</span>
              <span className="grow ellipsis">{w.title}</span>
              <span className="faint small">{w.project_name}</span>
              {w.due_date ? <span className={"small" + (isOverdue(w.due_date, w.completed_at) ? " overdue" : " faint")}>📅 {fmtDate(w.due_date, lang)}</span> : null}
            </Link>
          ))}
        </div>
      ) : null}
    </div>
  );
}
