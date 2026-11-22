// Property controls for one work item. Each opens a picker and saves through `onPatch`.
// Three looks, as in Plane: `chip` (24px outlined pill in list rows and board cards; an empty
// property is an icon-only square), `pill` (the same pill with a placeholder name, in the
// create modal) and the default borderless full-width field (detail view and sidebars).
import { Boxes, CalendarClock, CalendarDays, Contrast, Tag, Triangle, UsersRound } from "lucide-react";
import type { ReactNode } from "react";
import type { Cycle, Label, Module, Priority, Project, ProjMember, WfState, WorkItem } from "../api/types";
import { PRIORITIES } from "../api/types";
import { useI18n } from "../i18n";
import { Avatar } from "./Avatar";
import { Dot } from "./ColorPicker";
import { Picker } from "./Picker";
import { PriorityIcon, StateIcon, fmtDate, isOverdue } from "./Props";
import { DateTrigger } from "./DatePicker";

type Patch = (body: Partial<WorkItem>) => void;
export type PropVariant = "chip" | "pill" | "field";

export interface PropsCtx {
  project: Project;
  states: WfState[];
  labels: Label[];
  members: ProjMember[];
  cycles?: Cycle[];
  modules?: Module[];
  editable: boolean;
}

interface PropArgs {
  item: WorkItem;
  ctx: PropsCtx;
  onPatch: Patch;
  /** Shorthand for variant="chip". */
  chip?: boolean;
  variant?: PropVariant;
}

const variantOf = (a: PropArgs): PropVariant => a.variant ?? (a.chip ? "chip" : "field");
const cls = (v: PropVariant, empty: boolean, extra = "") =>
  (v === "field" ? "prop-btn" : "prop-chip" + (v === "pill" ? " prop-pill" : "") + (empty ? " is-empty" : "")) + (extra ? ` ${extra}` : "");

/** What a trigger shows: icon plus value; when empty, the placeholder (pill/field) or nothing (chip). */
function Face({ v, icon, value, placeholder }: { v: PropVariant; icon: ReactNode; value: ReactNode | null; placeholder: string }) {
  return (
    <>
      {icon}
      {value !== null ? value : v === "chip" ? null : <span className="prop-placeholder">{placeholder}</span>}
    </>
  );
}

export function StateProp(a: PropArgs) {
  const { item, ctx, onPatch } = a;
  const { t } = useI18n();
  const v = variantOf(a);
  const s = ctx.states.find((x) => x.id === item.state_id);
  return (
    <Picker
      label={t("wi.state")}
      title={s ? `${t("wi.state")}: ${s.name}` : t("wi.state")}
      className={cls(v, false)}
      disabled={!ctx.editable}
      options={ctx.states.map((x) => ({ value: x.id, label: x.name, icon: <StateIcon group={x.group} color={x.color} /> }))}
      value={[item.state_id]}
      onChange={(nv) => nv[0] && nv[0] !== item.state_id && onPatch({ state_id: nv[0] })}
      trigger={<Face v={v} icon={s ? <StateIcon group={s.group} color={s.color} /> : null} value={s ? <span className="ellipsis">{s.name}</span> : null} placeholder={t("wi.state")} />}
    />
  );
}

export function PriorityProp(a: PropArgs) {
  const { item, ctx, onPatch } = a;
  const { t } = useI18n();
  const v = variantOf(a);
  const none = item.priority === "none";
  return (
    <Picker
      label={t("wi.priority")}
      title={`${t("wi.priority")}: ${t(`prio.${item.priority}`)}`}
      className={cls(v, none, v === "chip" ? `prio-chip prio-${item.priority}` : "")}
      disabled={!ctx.editable}
      searchable={false}
      options={PRIORITIES.map((p) => ({ value: p, label: t(`prio.${p}`), icon: <PriorityIcon p={p} /> }))}
      value={[item.priority]}
      onChange={(nv) => nv[0] && onPatch({ priority: nv[0] as Priority })}
      trigger={<Face v={v} icon={<PriorityIcon p={item.priority} />} value={v === "chip" || none ? null : <span>{t(`prio.${item.priority}`)}</span>} placeholder={t("wi.priority")} />}
    />
  );
}

export function AssigneesProp(a: PropArgs) {
  const { item, ctx, onPatch } = a;
  const { t } = useI18n();
  const v = variantOf(a);
  const people = item.assignee_ids.map((id) => ctx.members.find((m) => m.user_id === id)).filter(Boolean) as ProjMember[];
  const names = people.map((m) => m.name || m.email).join(", ");
  const shown = people.length === 4 ? people : people.slice(0, 3);
  return (
    <Picker
      label={t("wi.assignees")}
      title={names ? `${t("wi.assignees")}: ${names}` : t("wi.assignees")}
      className={cls(v, !people.length, people.length && v === "chip" ? "prop-chip-bare" : "")}
      disabled={!ctx.editable}
      multi
      options={ctx.members.map((m) => ({ value: m.user_id, label: m.name || m.email, icon: <Avatar name={m.name} email={m.email} src={m.avatar} size={18} /> }))}
      value={item.assignee_ids}
      onChange={(nv) => onPatch({ assignee_ids: nv })}
      trigger={
        people.length ? (
          <>
            <span className="avatars">
              {shown.map((m) => (
                <Avatar key={m.user_id} name={m.name} email={m.email} src={m.avatar} size={v === "chip" ? 20 : 18} />
              ))}
              {people.length > shown.length ? <span className="avatar avatar-more">+{people.length - shown.length}</span> : null}
            </span>
            {v === "chip" ? null : <span className="ellipsis">{people.length === 1 ? names : t("wi.nAssignees", { n: people.length })}</span>}
          </>
        ) : (
          <Face v={v} icon={<UsersRound />} value={null} placeholder={v === "pill" ? t("wi.assignees") : t("wi.noAssignee")} />
        )
      }
    />
  );
}

export function LabelsProp(a: PropArgs) {
  const { item, ctx, onPatch } = a;
  const { t } = useI18n();
  const v = variantOf(a);
  const ls = item.label_ids.map((id) => ctx.labels.find((l) => l.id === id)).filter(Boolean) as Label[];
  const many = v === "chip" && ls.length > 3;
  return (
    <Picker
      label={t("wi.labels")}
      title={ls.length ? `${t("wi.labels")}: ${ls.map((l) => l.name).join(", ")}` : t("wi.labels")}
      className={cls(v, !ls.length, ls.length && v === "chip" ? "prop-chip-bare" : "")}
      disabled={!ctx.editable}
      multi
      options={ctx.labels.map((l) => ({ value: l.id, label: l.name, icon: <Dot color={l.color} /> }))}
      value={item.label_ids}
      onChange={(nv) => onPatch({ label_ids: nv })}
      trigger={
        ls.length ? (
          <span className="label-list">
            {(many ? ls.slice(0, 2) : ls).map((l) => (
              <span key={l.id} className="label-pill">
                <Dot color={l.color} size={8} />
                <span className="ellipsis">{l.name}</span>
              </span>
            ))}
            {many ? <span className="label-pill">{t("wi.moreLabels", { n: ls.length - 2 })}</span> : null}
          </span>
        ) : (
          <Face v={v} icon={<Tag />} value={null} placeholder={v === "pill" ? t("wi.labels") : t("wi.noLabels")} />
        )
      }
    />
  );
}

export function DateProp(a: PropArgs & { field: "start_date" | "due_date" }) {
  const { item, ctx, onPatch, field } = a;
  const { t, lang } = useI18n();
  const v = variantOf(a);
  const value = item[field];
  const overdue = field === "due_date" && isOverdue(value, item.completed_at);
  const label = t(field === "start_date" ? "wi.start" : "wi.due");
  return (
    <DateTrigger
      className={cls(v, !value, overdue ? "overdue" : "")}
      title={value ? `${label}: ${fmtDate(value, lang)}` : label}
      aria-label={label}
      disabled={!ctx.editable}
      value={value ?? null}
      kind={field === "due_date" ? "due" : "start"}
      completed={!!item.completed_at}
      onChange={(d) => onPatch({ [field]: d } as Partial<WorkItem>)}
    >
      <Face
        v={v}
        icon={field === "due_date" ? <CalendarClock /> : <CalendarDays />}
        value={value ? <span>{fmtDate(value, lang)}</span> : null}
        placeholder={v === "pill" ? label : t("wi.noDate")}
      />
    </DateTrigger>
  );
}

export function EstimateProp(a: PropArgs) {
  const { item, ctx, onPatch } = a;
  const { t } = useI18n();
  const v = variantOf(a);
  if (ctx.project.estimate_type === "none") return null;
  return (
    <Picker
      label={t("wi.estimate")}
      title={item.estimate ? `${t("wi.estimate")}: ${item.estimate}` : t("wi.estimate")}
      className={cls(v, !item.estimate)}
      disabled={!ctx.editable}
      searchable={false}
      allowClear={t("wi.noEstimate")}
      options={ctx.project.estimate_values.map((x) => ({ value: x, label: x }))}
      value={item.estimate ? [item.estimate] : []}
      onChange={(nv) => onPatch({ estimate: nv[0] ?? null })}
      trigger={<Face v={v} icon={<Triangle />} value={item.estimate ? <span>{item.estimate}</span> : null} placeholder={v === "pill" ? t("wi.estimate") : t("wi.noEstimate")} />}
    />
  );
}

export function CycleProp(a: PropArgs) {
  const { item, ctx, onPatch } = a;
  const { t } = useI18n();
  const v = variantOf(a);
  if (!ctx.project.feature_cycles) return null;
  const cycles = ctx.cycles ?? [];
  const cur = cycles.find((c) => c.id === item.cycle_id);
  // Completed cycles take no new items; the current one stays listed.
  const options = cycles
    .filter((c) => c.status !== "completed" || c.id === item.cycle_id)
    .map((c) => ({ value: c.id, label: c.name, hint: t(`cyc.status.${c.status}`) }));
  return (
    <Picker
      label={t("wi.cycle")}
      title={cur ? `${t("wi.cycle")}: ${cur.name}` : t("wi.cycle")}
      className={cls(v, !cur)}
      disabled={!ctx.editable}
      allowClear={t("wi.noCycle")}
      options={options}
      value={item.cycle_id ? [item.cycle_id] : []}
      onChange={(nv) => onPatch({ cycle_id: nv[0] ?? null })}
      trigger={<Face v={v} icon={<Contrast />} value={cur ? <span className="ellipsis">{cur.name}</span> : null} placeholder={v === "pill" ? t("wi.cycle") : t("wi.noCycle")} />}
    />
  );
}

export function ModulesProp(a: PropArgs) {
  const { item, ctx, onPatch } = a;
  const { t } = useI18n();
  const v = variantOf(a);
  if (!ctx.project.feature_modules) return null;
  const mods = ctx.modules ?? [];
  const ids = item.module_ids ?? [];
  const cur = ids.map((id) => mods.find((m) => m.id === id)).filter(Boolean) as Module[];
  const text = cur.length === 1 ? cur[0]!.name : cur.length ? t("wi.nModules", { n: cur.length }) : null;
  return (
    <Picker
      label={t("wi.modules")}
      title={cur.length ? `${t("wi.modules")}: ${cur.map((m) => m.name).join(", ")}` : t("wi.modules")}
      className={cls(v, !cur.length)}
      disabled={!ctx.editable}
      multi
      options={mods.filter((m) => !m.archived_at || ids.includes(m.id)).map((m) => ({ value: m.id, label: m.name, hint: t(`mod.st.${m.status}`) }))}
      value={ids}
      onChange={(nv) => onPatch({ module_ids: nv })}
      trigger={<Face v={v} icon={<Boxes />} value={text ? <span className="ellipsis">{text}</span> : null} placeholder={v === "pill" ? t("wi.modules") : t("wi.noModules")} />}
    />
  );
}
