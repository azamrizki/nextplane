import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Archive, CircleX } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { patch } from "../api/client";
import type { Project } from "../api/types";
import { Dialog } from "../components/Dialog";
import { StateIcon } from "../components/Props";
import { ErrorAlert } from "../components/ui";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { pcan } from "../lib/perm";
import { pq, projectPath, useStates } from "../lib/projects";
import { useWs } from "./WorkspaceHome";
import { NumberField } from "../components/NumberField";
import { Select } from "../components/Select";

const PRESETS = [1, 3, 6, 9, 12];

/** Project settings › Automations (as in Plane): archive closed work items and close inactive
 * ones after some months. Every change is saved at once; project admins only. */
export function Automations({ p }: { p: Project }) {
  const { t } = useI18n();
  const ws = useWs();
  const qc = useQueryClient();
  const states = useStates(ws.slug, p.id);
  const editable = pcan.editSettings(p) && !p.archived_at;
  // Shown at once, then confirmed (or put back) by the server's answer.
  const [cur, setCur] = useState({ archive_in: p.archive_in, close_in: p.close_in, close_state_id: p.close_state_id });
  useEffect(() => setCur({ archive_in: p.archive_in, close_in: p.close_in, close_state_id: p.close_state_id }), [p.archive_in, p.close_in, p.close_state_id]);
  const mutation = useMutation({
    mutationFn: (body: Partial<Project>) => patch<Project>(projectPath(ws.slug, p.id), body),
    onSuccess: (np) => {
      qc.setQueryData(pq.one(ws.slug, p.id), np);
      qc.invalidateQueries({ queryKey: pq.all(ws.slug) });
    },
    onError: () => setCur({ archive_in: p.archive_in, close_in: p.close_in, close_state_id: p.close_state_id }),
  });
  const save = {
    ...mutation,
    mutate: (body: Partial<Project>) => {
      setCur((c) => ({ ...c, ...body }));
      mutation.mutate(body);
    },
  };
  const cancelled = (states.data ?? []).filter((s) => s.group === "cancelled");
  const target = cancelled.find((s) => s.id === cur.close_state_id) ?? cancelled[0];
  return (
    <section className="automations">
      <p className="muted settings-lead">{t("auto.body")}</p>
      {!pcan.editSettings(p) ? <div className="alert alert-warning">{t("auto.adminsOnly")}</div> : null}
      <ErrorAlert message={save.isError ? errorText(save.error, t) : null} />
      <AutomationBox
        icon={<Archive />}
        title={t("auto.archiveTitle")}
        hint={t("auto.archiveHint")}
        on={cur.archive_in > 0}
        disabled={!editable}
        onToggle={(on) => save.mutate({ archive_in: on ? 1 : 0 })}
      >
        <div className="automation-row">
          <span>{t("auto.archiveAfter")}</span>
          <Months value={cur.archive_in} disabled={!editable} onChange={(n) => save.mutate({ archive_in: n })} />
        </div>
      </AutomationBox>
      <AutomationBox
        icon={<CircleX />}
        title={t("auto.closeTitle")}
        hint={t("auto.closeHint")}
        on={cur.close_in > 0}
        disabled={!editable || cancelled.length === 0}
        onToggle={(on) => save.mutate(on ? { close_in: 1 } : { close_in: 0 })}
      >
        <div className="automation-row">
          <span>{t("auto.closeAfter")}</span>
          <Months value={cur.close_in} disabled={!editable} onChange={(n) => save.mutate({ close_in: n })} />
        </div>
        <div className="automation-row">
          <span>{t("auto.closeState")}</span>
          <span className="automation-state">
            {target ? <StateIcon group={target.group} color={target.color} /> : null}
            <Select
              aria-label={t("auto.closeState")}
              value={target?.id ?? ""}
              disabled={!editable || cancelled.length < 2}
              onChange={(v) => save.mutate({ close_state_id: v })}
            >
              {cancelled.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </Select>
          </span>
        </div>
      </AutomationBox>
      {cancelled.length === 0 && states.isSuccess ? <p className="muted small">{t("auto.noCancelled")}</p> : null}
    </section>
  );
}

function AutomationBox({ icon, title, hint, on, disabled, onToggle, children }: { icon: ReactNode; title: string; hint: string; on: boolean; disabled: boolean; onToggle: (on: boolean) => void; children: ReactNode }) {
  return (
    <div className="card automation">
      <label className="automation-head">
        <span className="automation-icon" aria-hidden>
          {icon}
        </span>
        <span className="row-main">
          <b>{title}</b>
          <span className="muted small">{hint}</span>
        </span>
        <input type="checkbox" role="switch" className="switch" checked={on} disabled={disabled} onChange={(e) => onToggle(e.target.checked)} aria-label={title} />
      </label>
      {on ? <div className="automation-body">{children}</div> : null}
    </div>
  );
}

/** 1, 3, 6, 9 or 12 months, or a custom number from 1 to 12. */
function Months({ value, disabled, onChange }: { value: number; disabled: boolean; onChange: (n: number) => void }) {
  const { t } = useI18n();
  const [custom, setCustom] = useState(false);
  const [n, setN] = useState(String(value || 1));
  const options = PRESETS.includes(value) ? PRESETS : [...PRESETS, value].sort((a, b) => a - b);
  const valid = /^\d+$/.test(n) && +n >= 1 && +n <= 12;
  return (
    <>
      <Select
        aria-label={t("auto.months")}
        value={String(value)}
        disabled={disabled}
        onChange={(v) => (v === "custom" ? (setN(String(value || 1)), setCustom(true)) : onChange(+v))}
      >
        {options.map((m) => (
          <option key={m} value={m}>
            {t(m === 1 ? "auto.month" : "auto.monthsN", { n: m })}
          </option>
        ))}
        <option value="custom">{t("auto.custom")}</option>
      </Select>
      <Dialog
        open={custom}
        onClose={() => setCustom(false)}
        title={t("auto.customTitle")}
        width={360}
        footer={
          <>
            <button className="btn" onClick={() => setCustom(false)}>
              {t("common.cancel")}
            </button>
            <button className="btn btn-primary" disabled={!valid} onClick={() => (onChange(+n), setCustom(false))}>
              {t("common.save")}
            </button>
          </>
        }
      >
        <label className="field">
          <span>{t("auto.months")}</span>
          <NumberField min={1} max={12} value={n} onChange={setN} autoFocus />
          {!valid ? <span className="small" style={{ color: "var(--danger)" }}>{t("auto.range")}</span> : null}
        </label>
      </Dialog>
    </>
  );
}
