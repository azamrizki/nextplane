// Property controls for one work item. Each opens a picker and saves through `onPatch`.
import type { Cycle, Label, Module, Priority, Project, ProjMember, WfState, WorkItem } from "../api/types";
import { PRIORITIES } from "../api/types";
import { useI18n } from "../i18n";
import { Avatar } from "./Avatar";
import { Dot } from "./ColorPicker";
import { Picker } from "./Picker";
import { PriorityIcon, StateIcon, fmtDate, isOverdue } from "./Props";

type Patch = (body: Partial<WorkItem>) => void;

export interface PropsCtx {
  project: Project;
  states: WfState[];
  labels: Label[];
  members: ProjMember[];
  cycles?: Cycle[];
  modules?: Module[];
  editable: boolean;
}

export function StateProp({ item, ctx, onPatch, chip }: { item: WorkItem; ctx: PropsCtx; onPatch: Patch; chip?: boolean }) {
  const { t } = useI18n();
  const s = ctx.states.find((x) => x.id === item.state_id);
  return (
    <Picker
      label={t("wi.state")}
      className={chip ? "prop-chip" : "prop-btn"}
      disabled={!ctx.editable}
      options={ctx.states.map((x) => ({ value: x.id, label: x.name, icon: <StateIcon group={x.group} color={x.color} /> }))}
      value={[item.state_id]}
      onChange={(v) => v[0] && v[0] !== item.state_id && onPatch({ state_id: v[0] })}
      trigger={
        <>
          {s ? <StateIcon group={s.group} color={s.color} /> : null}
          {chip ? null : <span className="ellipsis">{s?.name}</span>}
        </>
      }
    />
  );
}

export function PriorityProp({ item, ctx, onPatch, chip }: { item: WorkItem; ctx: PropsCtx; onPatch: Patch; chip?: boolean }) {
  const { t } = useI18n();
  return (
    <Picker
      label={t("wi.priority")}
      className={chip ? "prop-chip" : "prop-btn"}
      disabled={!ctx.editable}
      searchable={false}
      options={PRIORITIES.map((p) => ({ value: p, label: t(`prio.${p}`), icon: <PriorityIcon p={p} /> }))}
      value={[item.priority]}
      onChange={(v) => v[0] && onPatch({ priority: v[0] as Priority })}
      trigger={
        <>
          <PriorityIcon p={item.priority} />
          {chip ? null : <span>{t(`prio.${item.priority}`)}</span>}
        </>
      }
    />
  );
}

export function AssigneesProp({ item, ctx, onPatch, chip }: { item: WorkItem; ctx: PropsCtx; onPatch: Patch; chip?: boolean }) {
  const { t } = useI18n();
  const people = item.assignee_ids.map((id) => ctx.members.find((m) => m.user_id === id)).filter(Boolean) as ProjMember[];
  return (
    <Picker
      label={t("wi.assignees")}
      className={chip ? "prop-chip prop-chip-plain" : "prop-btn"}
      disabled={!ctx.editable}
      multi
      options={ctx.members.map((m) => ({ value: m.user_id, label: m.name || m.email, icon: <Avatar name={m.name} email={m.email} src={m.avatar} size={18} /> }))}
      value={item.assignee_ids}
      onChange={(v) => onPatch({ assignee_ids: v })}
      trigger={
        people.length ? (
          <>
            <span className="avatars">
              {people.slice(0, 3).map((m) => (
                <Avatar key={m.user_id} name={m.name} email={m.email} src={m.avatar} size={18} />
              ))}
            </span>
            {chip ? null : <span className="ellipsis">{people.length === 1 ? people[0]!.name || people[0]!.email : `${people.length}`}</span>}
          </>
        ) : (
          <span className="faint">{chip ? "👤" : t("wi.noAssignee")}</span>
        )
      }
    />
  );
}

export function LabelsProp({ item, ctx, onPatch, chip }: { item: WorkItem; ctx: PropsCtx; onPatch: Patch; chip?: boolean }) {
  const { t } = useI18n();
  const ls = item.label_ids.map((id) => ctx.labels.find((l) => l.id === id)).filter(Boolean) as Label[];
  return (
    <Picker
      label={t("wi.labels")}
      className={chip ? "prop-chip" + (ls.length ? "" : " is-empty") : "prop-btn"}
      disabled={!ctx.editable}
      multi
      options={ctx.labels.map((l) => ({ value: l.id, label: l.name, icon: <Dot color={l.color} /> }))}
      value={item.label_ids}
      onChange={(v) => onPatch({ label_ids: v })}
      trigger={
        ls.length ? (
          <span className="label-list">
            {ls.slice(0, chip ? 2 : 6).map((l) => (
              <span key={l.id} className="label-pill">
                <Dot color={l.color} size={8} />
                {l.name}
              </span>
            ))}
            {chip && ls.length > 2 ? <span className="faint">+{ls.length - 2}</span> : null}
          </span>
        ) : chip ? (
          <span className="faint">🏷</span>
        ) : (
          <span className="faint">{t("wi.noLabels")}</span>
        )
      }
    />
  );
}

export function DateProp({ item, field, ctx, onPatch, chip }: { item: WorkItem; field: "start_date" | "due_date"; ctx: PropsCtx; onPatch: Patch; chip?: boolean }) {
  const { t, lang } = useI18n();
  const v = item[field];
  const overdue = field === "due_date" && isOverdue(v, item.completed_at);
  const label = t(field === "start_date" ? "wi.start" : "wi.due");
  if (chip && !v) return null;
  return (
    <label className={(chip ? "prop-chip" : "prop-btn") + (overdue ? " overdue" : "")} title={label}>
      <span aria-hidden>{field === "due_date" ? "📅" : "▶"}</span>
      <span className={v ? "" : "faint"}>{v ? fmtDate(v, lang) : t("wi.noDate")}</span>
      <input
        type="date"
        className="date-overlay"
        aria-label={label}
        disabled={!ctx.editable}
        value={v ?? ""}
        onChange={(e) => onPatch({ [field]: e.target.value || null } as Partial<WorkItem>)}
      />
    </label>
  );
}

export function EstimateProp({ item, ctx, onPatch, chip }: { item: WorkItem; ctx: PropsCtx; onPatch: Patch; chip?: boolean }) {
  const { t } = useI18n();
  if (ctx.project.estimate_type === "none") return null;
  if (chip && !item.estimate) return null;
  return (
    <Picker
      label={t("wi.estimate")}
      className={chip ? "prop-chip" : "prop-btn"}
      disabled={!ctx.editable}
      searchable={false}
      allowClear={t("wi.noEstimate")}
      options={ctx.project.estimate_values.map((v) => ({ value: v, label: v }))}
      value={item.estimate ? [item.estimate] : []}
      onChange={(v) => onPatch({ estimate: v[0] ?? null })}
      trigger={<span className={item.estimate ? "" : "faint"}>{item.estimate ? `◷ ${item.estimate}` : t("wi.noEstimate")}</span>}
    />
  );
}

export function CycleProp({ item, ctx, onPatch, chip }: { item: WorkItem; ctx: PropsCtx; onPatch: Patch; chip?: boolean }) {
  const { t } = useI18n();
  if (!ctx.project.feature_cycles) return null;
  const cycles = ctx.cycles ?? [];
  const cur = cycles.find((c) => c.id === item.cycle_id);
  if (chip && !cur) return null;
  // Completed cycles take no new items; the current one stays listed.
  const options = cycles
    .filter((c) => c.status !== "completed" || c.id === item.cycle_id)
    .map((c) => ({ value: c.id, label: c.name, hint: t(`cyc.status.${c.status}`) }));
  return (
    <Picker
      label={t("wi.cycle")}
      className={chip ? "prop-chip" : "prop-btn"}
      disabled={!ctx.editable}
      allowClear={t("wi.noCycle")}
      options={options}
      value={item.cycle_id ? [item.cycle_id] : []}
      onChange={(v) => onPatch({ cycle_id: v[0] ?? null })}
      trigger={<span className={cur ? "ellipsis" : "faint"}>{cur ? `↻ ${cur.name}` : t("wi.noCycle")}</span>}
    />
  );
}

export function ModulesProp({ item, ctx, onPatch, chip }: { item: WorkItem; ctx: PropsCtx; onPatch: Patch; chip?: boolean }) {
  const { t } = useI18n();
  if (!ctx.project.feature_modules) return null;
  const mods = ctx.modules ?? [];
  const ids = item.module_ids ?? [];
  const cur = ids.map((id) => mods.find((m) => m.id === id)).filter(Boolean) as Module[];
  if (chip && !cur.length) return null;
  return (
    <Picker
      label={t("wi.modules")}
      className={chip ? "prop-chip" : "prop-btn"}
      disabled={!ctx.editable}
      multi
      options={mods.filter((m) => !m.archived_at || ids.includes(m.id)).map((m) => ({ value: m.id, label: m.name, hint: t(`mod.st.${m.status}`) }))}
      value={ids}
      onChange={(v) => onPatch({ module_ids: v })}
      trigger={<span className={cur.length ? "ellipsis" : "faint"}>{cur.length ? `▦ ${cur.map((m) => m.name).join(", ")}` : t("wi.noModules")}</span>}
    />
  );
}
