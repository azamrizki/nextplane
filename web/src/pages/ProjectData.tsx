// Project settings: import & export (CSV/JSON) and the public board (SPRINT S3.5).
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { get, post, postText } from "../api/client";
import type { Project } from "../api/types";
import { ErrorAlert } from "../components/ui";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { pcan } from "../lib/perm";
import { projectPath } from "../lib/projects";
import { useWs } from "./WorkspaceHome";

const FIELDS = ["title", "description", "state", "priority", "assignees", "labels", "start_date", "due_date", "estimate"] as const;
type Field = (typeof FIELDS)[number];

interface Preview {
  headers: string[];
  rows: string[][];
  total_rows: number;
  mapping: Partial<Record<Field, number>>;
}
interface Result {
  created: number;
  skipped: { line: number; message: string }[];
  warnings: { line: number; message: string }[];
}

export function DataTab({ p }: { p: Project }) {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const base = projectPath(ws.slug, p.id);
  const [csv, setCsv] = useState("");
  const [preview, setPreview] = useState<Preview | null>(null);
  const [mapping, setMapping] = useState<Partial<Record<Field, number>>>({});
  const [result, setResult] = useState<Result | null>(null);
  const load = useMutation({
    mutationFn: (text: string) => postText<Preview>(`${base}/import/preview`, text),
    onSuccess: (pv) => {
      setPreview(pv);
      setMapping(pv.mapping);
      setResult(null);
    },
  });
  const run = useMutation({
    mutationFn: () => post<Result>(`${base}/import`, { csv, mapping }),
    onSuccess: (r) => {
      setResult(r);
      setPreview(null);
      qc.invalidateQueries({ queryKey: ["work-items", ws.slug, p.id] });
    },
  });
  const canImport = pcan.createWorkItem(p) && !p.archived_at;
  return (
    <>
      <section className="card">
        <div className="card-head">{t("x.export")}</div>
        <div className="card-body form">
          <p className="muted small">{t("x.exportHint")}</p>
          <div className="page-actions">
            <a className="btn" href={`/api/v1${base}/export?format=csv`} download>{t("x.csv")}</a>
            <a className="btn" href={`/api/v1${base}/export?format=json`} download>{t("x.json")}</a>
          </div>
        </div>
      </section>
      <section className="card">
        <div className="card-head">{t("x.import")}</div>
        <div className="card-body form">
          {!canImport ? (
            <p className="muted small">{t("x.noImport")}</p>
          ) : (
            <>
              <p className="muted small">{t("x.importHint")}</p>
              <ErrorAlert message={load.error ? errorText(load.error, t) : run.error ? errorText(run.error, t) : null} />
              <label className="btn" style={{ alignSelf: "flex-start" }}>
                {t("x.pick")}
                <input
                  type="file"
                  accept=".csv,text/csv"
                  hidden
                  aria-label={t("x.pick")}
                  onChange={async (e) => {
                    const f = e.target.files?.[0];
                    if (!f) return;
                    const text = await f.text();
                    setCsv(text);
                    load.mutate(text);
                    e.target.value = "";
                  }}
                />
              </label>
              {preview ? (
                <>
                  <p className="small">{t("x.rows", { n: preview.total_rows })}</p>
                  <div className="chart-table-wrap">
                    <table className="data-table">
                      <thead>
                        <tr>{preview.headers.map((h, i) => <th key={i} scope="col">{h}</th>)}</tr>
                      </thead>
                      <tbody>
                        {preview.rows.map((r, i) => (
                          <tr key={i}>{preview.headers.map((_, j) => <td key={j}>{(r[j] ?? "").slice(0, 80)}</td>)}</tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                  <table className="data-table map-table">
                    <thead>
                      <tr>
                        <th scope="col">{t("x.field")}</th>
                        <th scope="col">{t("x.column")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {FIELDS.map((f) => (
                        <tr key={f}>
                          <th scope="row">
                            <label htmlFor={`map-${f}`}>{t(`x.f.${f}`)}</label>
                          </th>
                          <td>
                            <select
                              id={`map-${f}`}
                              className="select"
                              value={mapping[f] ?? ""}
                              onChange={(e) => setMapping((m) => ({ ...m, [f]: e.target.value === "" ? undefined : Number(e.target.value) }))}
                            >
                              <option value="">{t("x.noColumn")}</option>
                              {preview.headers.map((h, i) => (
                                <option key={i} value={i}>{h || `#${i + 1}`}</option>
                              ))}
                            </select>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {mapping.title === undefined ? <div className="alert alert-warning">{t("x.needTitle")}</div> : null}
                  <div>
                    <button className="btn btn-primary" disabled={run.isPending || mapping.title === undefined} onClick={() => run.mutate()}>
                      {t("x.run", { n: preview.total_rows })}
                    </button>
                  </div>
                </>
              ) : null}
              {result ? (
                <div className="form" role="status">
                  <div className="alert alert-success">{t("x.done", { n: result.created })}</div>
                  {result.skipped.length ? <Notes title={t("x.skipped")} list={result.skipped} /> : null}
                  {result.warnings.length ? <Notes title={t("x.warnings")} list={result.warnings} /> : null}
                </div>
              ) : null}
            </>
          )}
        </div>
      </section>
    </>
  );
}

function Notes({ title, list }: { title: string; list: { line: number; message: string }[] }) {
  const { t } = useI18n();
  return (
    <details open={list.length <= 10}>
      <summary className="small">{title} ({list.length})</summary>
      <ul className="small notes-list">
        {list.slice(0, 200).map((n, i) => (
          <li key={i}>
            <span className="faint">{t("x.line", { n: n.line })}</span> {n.message}
          </li>
        ))}
      </ul>
    </details>
  );
}

export function PublishTab({ p }: { p: Project }) {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const base = `${projectPath(ws.slug, p.id)}/publish`;
  const key = ["publish", ws.slug, p.id];
  const admin = pcan.editSettings(p);
  const q = useQuery({ queryKey: key, queryFn: () => get<{ enabled: boolean; path: string | null }>(base), enabled: admin });
  const set = useMutation({ mutationFn: (enabled: boolean) => post<{ enabled: boolean; path: string | null }>(base, { enabled }), onSuccess: (v) => qc.setQueryData(key, v) });
  const [copied, setCopied] = useState(false);
  const url = q.data?.path ? `${location.origin}${q.data.path}` : "";
  return (
    <section className="card">
      <div className="card-head">{t("pub.title")}</div>
      <div className="card-body form">
        {!admin ? <p className="muted small">{t("pub.adminOnly")}</p> : null}
        <ErrorAlert message={set.error ? errorText(set.error, t) : null} />
        {q.data?.enabled ? (
          <>
            <p className="small">{t("pub.on")}</p>
            <div className="row-flex copy-row">
              <input className="input mono small" readOnly value={url} aria-label={t("pub.title")} onFocus={(e) => e.target.select()} />
              <a className="btn btn-sm" href={url} target="_blank" rel="noreferrer">↗</a>
              <button
                className="btn btn-sm"
                onClick={async () => {
                  await navigator.clipboard?.writeText(url).catch(() => {});
                  setCopied(true);
                }}
              >
                {copied ? t("in.copied") : t("in.copy")}
              </button>
            </div>
            <div className="page-actions">
              <button className="btn btn-sm" onClick={() => set.mutate(true)}>{t("pub.renew")}</button>
              <button className="btn btn-sm btn-ghost danger-text" onClick={() => set.mutate(false)}>{t("pub.disable")}</button>
            </div>
          </>
        ) : q.isSuccess ? (
          <>
            <p className="muted small">{t("pub.off")}</p>
            <div>
              <button className="btn btn-primary" onClick={() => set.mutate(true)}>{t("pub.enable")}</button>
            </div>
          </>
        ) : null}
      </div>
    </section>
  );
}
