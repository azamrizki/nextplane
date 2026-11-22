import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Download, FileDown, RefreshCw } from "lucide-react";
import { useMemo, useState } from "react";
import { Navigate } from "react-router";
import { get, post } from "../api/client";
import { Dialog } from "../components/Dialog";
import { Picker } from "../components/Picker";
import { WsSettingsFrame } from "../components/SettingsFrame";
import { fmtDate } from "../components/Props";
import { ErrorAlert } from "../components/ui";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { pcan } from "../lib/perm";
import { useProjects } from "../lib/projects";
import { toast } from "../lib/toast";
import { useWs } from "./WorkspaceHome";
import { Select } from "../components/Select";

interface ExportEntry {
  id: string;
  format: "csv" | "json" | "xlsx";
  project_ids: string[];
  items: number;
  created_at: string;
}

const FORMATS = ["csv", "xlsx", "json"] as const;

/** Workspace settings › Exports (as in Plane): work items of several projects as CSV, Excel or
 * JSON, and your previous exports to download again. */
export function ExportsPage() {
  const { t, lang } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const [page, setPage] = useState(1);
  const [open, setOpen] = useState(false);
  const list = useQuery({ queryKey: ["exports", ws.slug, page], queryFn: () => get<ExportEntry[]>(`/workspaces/${ws.slug}/exports?page=${page}`), enabled: ws.role !== "guest" });
  const projects = useProjects(ws.slug);
  const names = useMemo(() => new Map((projects.data ?? []).map((p) => [p.id, p.name])), [projects.data]);
  if (ws.role === "guest") return <Navigate to={`/${ws.slug}/settings`} replace />;
  const download = (e: ExportEntry) => window.location.assign(`/api/v1/workspaces/${ws.slug}/exports/${e.id}/download`);
  return (
    <WsSettingsFrame
      ws={ws}
      crumbs={[{ icon: <FileDown />, label: t("ex.title") }]}
      actions={
        <button className="btn btn-primary" onClick={() => setOpen(true)}>
          {t("ex.export")}
        </button>
      }
    >
      <p className="muted settings-lead">{t("ex.body")}</p>
      <div className="row-flex" style={{ justifyContent: "space-between" }}>
        <h3 className="section-title">{t("ex.previous")}</h3>
        <button className="btn btn-sm btn-ghost" onClick={() => qc.invalidateQueries({ queryKey: ["exports", ws.slug] })}>
          <RefreshCw />
          {t("ex.refresh")}
        </button>
      </div>
      {list.isSuccess && !list.data.length && page === 1 ? <div className="card empty small">{t("ex.empty")}</div> : null}
      {list.data?.length ? (
        <table className="data-table exports-table">
          <thead>
            <tr>
              <th>{t("ex.when")}</th>
              <th>{t("ex.projects")}</th>
              <th>{t("ex.format")}</th>
              <th>{t("ex.items")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {list.data.map((e) => (
              <tr key={e.id}>
                <td>{fmtDate(e.created_at.slice(0, 10), lang)} {e.created_at.slice(11, 16)}</td>
                <td className="ellipsis">{e.project_ids.length ? e.project_ids.map((id) => names.get(id) ?? "—").join(", ") : t("ex.allProjects")}</td>
                <td>{t(`ex.f.${e.format}`)}</td>
                <td>{e.items}</td>
                <td>
                  <button className="btn btn-sm" onClick={() => download(e)}>
                    <Download />
                    {t("ex.download")}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
      {page > 1 || (list.data?.length ?? 0) === 10 ? (
        <div className="row-flex">
          <button className="btn btn-sm" disabled={page === 1} onClick={() => setPage((p) => p - 1)}>
            {t("ex.newer")}
          </button>
          <button className="btn btn-sm" disabled={(list.data?.length ?? 0) < 10} onClick={() => setPage((p) => p + 1)}>
            {t("ex.older")}
          </button>
        </div>
      ) : null}
      {open ? (
        <ExportDialog
          onClose={() => setOpen(false)}
          onDone={(e) => {
            setOpen(false);
            setPage(1);
            qc.invalidateQueries({ queryKey: ["exports", ws.slug] });
            toast({ kind: "success", title: t("ex.ready", { n: e.items }) });
            download(e);
          }}
        />
      ) : null}
    </WsSettingsFrame>
  );
}

function ExportDialog({ onClose, onDone }: { onClose: () => void; onDone: (e: ExportEntry) => void }) {
  const { t } = useI18n();
  const ws = useWs();
  const projects = useProjects(ws.slug);
  const mine = (projects.data ?? []).filter((p) => p.is_member && !p.archived_at && pcan.createWorkItem(p));
  const [chosen, setChosen] = useState<string[]>([]);
  const [format, setFormat] = useState<(typeof FORMATS)[number]>("csv");
  const run = useMutation({
    mutationFn: () => post<ExportEntry>(`/workspaces/${ws.slug}/exports`, { format, project_ids: chosen }),
    onSuccess: onDone,
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("ex.export")}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button className="btn btn-primary" disabled={run.isPending || !mine.length} onClick={() => run.mutate()}>
            {t("ex.export")}
          </button>
        </>
      }
    >
      <ErrorAlert message={run.isError ? errorText(run.error, t) : null} />
      <div className="form">
        <div className="field">
          <label>{t("ex.projects")}</label>
          <Picker
            label={t("ex.projects")}
            className="btn btn-block select-like"
            multi
            value={chosen}
            onChange={setChosen}
            options={mine.map((p) => ({ value: p.id, label: p.name, hint: p.identifier }))}
            trigger={<span className="ellipsis">{chosen.length ? chosen.map((id) => mine.find((p) => p.id === id)?.name).join(", ") : t("ex.allProjects")}</span>}
          />
        </div>
        <div className="field">
          <label htmlFor="exf">{t("ex.format")}</label>
          <Select id="exf" value={format} onChange={(v) => setFormat(v as (typeof FORMATS)[number])}>
            {FORMATS.map((f) => (
              <option key={f} value={f}>
                {t(`ex.f.${f}`)}
              </option>
            ))}
          </Select>
        </div>
        <p className="muted small">{t("ex.what")}</p>
      </div>
    </Dialog>
  );
}
