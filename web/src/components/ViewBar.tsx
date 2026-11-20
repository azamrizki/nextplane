import { useState, type ReactNode } from "react";
import type { Priority } from "../api/types";
import { PRIORITIES } from "../api/types";
import { useI18n } from "../i18n";
import { Popover } from "./Layout";
import { Avatar } from "./Avatar";
import { Dot } from "./ColorPicker";
import { Picker } from "./Picker";
import { PriorityIcon, StateIcon } from "./Props";
import type { PropsCtx } from "./WorkItemProps";
import { DISPLAY_KEYS, EMPTY_FILTERS, LAYOUTS, activeFilterCount, type DueFilter, type Filters, type GroupBy, type OrderBy, type ViewState } from "../lib/view";

export function ViewBar({ view, setView, ctx, views }: { view: ViewState; setView: (p: Partial<ViewState>) => void; ctx: PropsCtx; views?: ReactNode }) {
  const { t } = useI18n();
  const f = view.filters;
  const setF = (p: Partial<Filters>) => setView({ filters: { ...f, ...p } });
  const people = [...ctx.members.map((m) => ({ value: m.user_id, label: m.name || m.email, icon: <Avatar name={m.name} email={m.email} src={m.avatar} size={18} /> }))];
  const n = activeFilterCount(f);
  return (
    <div className="view-bar">
      {views ?? null}
      <input className="input view-search" placeholder={t("view.search")} aria-label={t("view.search")} value={f.q} onChange={(e) => setF({ q: e.target.value })} />
      <div className="seg" role="tablist" aria-label={t("view.display")}>
        {LAYOUTS.map((l) => (
          <button key={l} role="tab" aria-selected={view.layout === l} aria-label={t(`view.${l}`)} title={t(`view.${l}`)} className={"layout-tab" + (view.layout === l ? " on" : "")} onClick={() => setView({ layout: l })}>
            <LayoutIcon layout={l} />
          </button>
        ))}
      </div>
      <span className="grow" />
      <Picker label={t("view.filterBy", { name: t("filter.state") })} className="btn btn-sm" multi value={f.state} onChange={(v) => setF({ state: v })}
        options={ctx.states.map((s) => ({ value: s.id, label: s.name, icon: <StateIcon group={s.group} color={s.color} /> }))}
        trigger={<>{t("filter.state")}{f.state.length ? <span className="count">{f.state.length}</span> : null}</>} align="right" />
      <Picker label={t("view.filterBy", { name: t("filter.priority") })} className="btn btn-sm" multi searchable={false} value={f.priority} onChange={(v) => setF({ priority: v as Priority[] })}
        options={PRIORITIES.map((p) => ({ value: p, label: t(`prio.${p}`), icon: <PriorityIcon p={p} /> }))}
        trigger={<>{t("filter.priority")}{f.priority.length ? <span className="count">{f.priority.length}</span> : null}</>} align="right" />
      <Picker label={t("view.filterBy", { name: t("filter.assignee") })} className="btn btn-sm" multi value={f.assignee} onChange={(v) => setF({ assignee: v })}
        options={[{ value: "none", label: t("noAssignee") }, ...people]}
        trigger={<>{t("filter.assignee")}{f.assignee.length ? <span className="count">{f.assignee.length}</span> : null}</>} align="right" />
      <Picker label={t("view.filterBy", { name: t("filter.label") })} className="btn btn-sm" multi value={f.label} onChange={(v) => setF({ label: v })}
        options={ctx.labels.map((l) => ({ value: l.id, label: l.name, icon: <Dot color={l.color} /> }))}
        trigger={<>{t("filter.label")}{f.label.length ? <span className="count">{f.label.length}</span> : null}</>} align="right" />
      {ctx.project.feature_cycles && ctx.cycles ? (
        <Picker label={t("view.filterBy", { name: t("filter.cycle") })} className="btn btn-sm" multi value={f.cycle} onChange={(v) => setF({ cycle: v })}
          options={[{ value: "none", label: t("noCycle") }, ...ctx.cycles.map((c) => ({ value: c.id, label: c.name, hint: t(`cyc.status.${c.status}`) }))]}
          trigger={<>{t("filter.cycle")}{f.cycle.length ? <span className="count">{f.cycle.length}</span> : null}</>} align="right" />
      ) : null}
      {ctx.project.feature_modules && ctx.modules ? (
        <Picker label={t("view.filterBy", { name: t("filter.module") })} className="btn btn-sm" multi value={f.module} onChange={(v) => setF({ module: v })}
          options={[{ value: "none", label: t("noModule") }, ...ctx.modules.map((m) => ({ value: m.id, label: m.name }))]}
          trigger={<>{t("filter.module")}{f.module.length ? <span className="count">{f.module.length}</span> : null}</>} align="right" />
      ) : null}
      <Picker label={t("view.filterBy", { name: t("filter.due") })} className="btn btn-sm" searchable={false} value={f.due ? [f.due] : []} allowClear={t("due.any")} onChange={(v) => setF({ due: (v[0] ?? "") as DueFilter })}
        options={[{ value: "overdue", label: t("due.overdue") }, { value: "week", label: t("due.week") }]}
        trigger={<>{f.due ? t(`due.${f.due}` as "due.overdue") : t("filter.due")}</>} align="right" />
      <Picker label={t("view.filterBy", { name: t("filter.created_by") })} className="btn btn-sm" multi value={f.created_by} onChange={(v) => setF({ created_by: v })}
        options={people} trigger={<>{t("filter.created_by")}{f.created_by.length ? <span className="count">{f.created_by.length}</span> : null}</>} align="right" />
      {n ? <button className="btn btn-sm btn-ghost" onClick={() => setView({ filters: { ...EMPTY_FILTERS, q: f.q } })}>{t("view.clear")}</button> : null}
      <DisplayMenu view={view} setView={setView} />
    </div>
  );
}

function DisplayMenu({ view, setView }: { view: ViewState; setView: (p: Partial<ViewState>) => void }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  return (
    <Popover
      open={open}
      onClose={() => setOpen(false)}
      trigger={<button className="btn btn-sm" onClick={() => setOpen((o) => !o)} aria-expanded={open}>{t("view.display")}</button>}
    >
      <div className="menu display-menu" style={{ right: 0 }}>
        <label className="field">
          <span className="small muted">{t("view.groupBy")}</span>
          <select className="select select-inline" value={view.groupBy} onChange={(e) => setView({ groupBy: e.target.value as GroupBy })}>
            {(["state", "priority", "assignee", "label", "none"] as GroupBy[]).map((g) => <option key={g} value={g}>{t(`groupBy.${g}`)}</option>)}
          </select>
        </label>
        <label className="field">
          <span className="small muted">{t("view.orderBy")}</span>
          <select className="select select-inline" value={view.orderBy} onChange={(e) => setView({ orderBy: e.target.value as OrderBy })}>
            {(["manual", "priority", "due_date", "created_at", "updated_at"] as OrderBy[]).map((o) => <option key={o} value={o}>{t(`orderBy.${o}`)}</option>)}
          </select>
        </label>
        <label className="checkbox small"><input type="checkbox" checked={view.showSubItems} onChange={(e) => setView({ showSubItems: e.target.checked })} />{t("view.subItems")}</label>
        <label className="checkbox small"><input type="checkbox" checked={view.showEmptyGroups} onChange={(e) => setView({ showEmptyGroups: e.target.checked })} />{t("view.emptyGroups")}</label>
        <div className="menu-sep" />
        <span className="small muted">{t("view.properties")}</span>
        <div className="toggle-chips">
          {DISPLAY_KEYS.map((k) => (
            <button key={k} type="button" className={"toggle-chip" + (view.display[k] ? " on" : "")} aria-pressed={view.display[k]} onClick={() => setView({ display: { ...view.display, [k]: !view.display[k] } })}>
              {t(`disp.${k}`)}
            </button>
          ))}
        </div>
      </div>
    </Popover>
  );
}

/** Small line icons for the layouts (decorative; the tab carries the name). */
function LayoutIcon({ layout }: { layout: string }) {
  const p = { fill: "none", stroke: "currentColor", strokeWidth: 1.5, strokeLinecap: "round" as const };
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden>
      {layout === "list" ? <path {...p} d="M3 4h10M3 8h10M3 12h10" /> : null}
      {layout === "kanban" ? <path {...p} d="M3 3v10M8 3v6M13 3v8" /> : null}
      {layout === "spreadsheet" ? <path {...p} d="M2.5 3.5h11v9h-11zM2.5 7h11M6.5 3.5v9" /> : null}
      {layout === "calendar" ? <path {...p} d="M2.5 4h11v9h-11zM2.5 7h11M5.5 2.5v3M10.5 2.5v3" /> : null}
      {layout === "gantt" ? <path {...p} d="M3 4h6M5 8h7M3 12h4" /> : null}
    </svg>
  );
}
