import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FilePen } from "lucide-react";
import { PageHeader } from "../components/PageHeader";
import { Link } from "react-router";
import { del, get, post } from "../api/client";
import type { WorkItem } from "../api/types";
import { PriorityIcon } from "../components/Props";
import { useI18n } from "../i18n";
import { itemsPath } from "../lib/workItems";
import { useWs } from "./WorkspaceHome";

interface Draft extends WorkItem {
  identifier: string;
  project_name: string;
}

export function DraftsPage() {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const key = ["drafts", ws.slug];
  const q = useQuery({ queryKey: key, queryFn: () => get<Draft[]>(`/workspaces/${ws.slug}/drafts`) });
  const refresh = () => {
    qc.invalidateQueries({ queryKey: key });
    qc.invalidateQueries({ queryKey: ["work-items", ws.slug] });
  };
  const publish = useMutation({ mutationFn: (d: Draft) => post(itemsPath(ws.slug, d.project_id, d.id) + "/publish"), onSuccess: refresh });
  const remove = useMutation({ mutationFn: (d: Draft) => del(itemsPath(ws.slug, d.project_id, d.id)), onSuccess: refresh });
  const rows = q.data ?? [];
  return (
    <div className="page page-wide">
      <PageHeader crumbs={[{ icon: <FilePen />, label: t("nav.drafts") }]} />
      {q.isSuccess && rows.length === 0 ? <div className="card empty">{t("drafts.empty")}</div> : null}
      {rows.length ? (
        <div className="rows card">
          {rows.map((d) => (
            <div key={d.id} className="row">
              <PriorityIcon p={d.priority} />
              <Link className="grow ellipsis row-link-inline" to={`/${ws.slug}/projects/${d.project_id}/work-items/${d.id}`}>{d.title}</Link>
              <span className="faint small">{d.project_name}</span>
              <button className="btn btn-sm btn-primary" onClick={() => publish.mutate(d)}>{t("draft.publish")}</button>
              <button className="btn btn-sm btn-ghost" onClick={() => remove.mutate(d)}>{t("common.delete")}</button>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
