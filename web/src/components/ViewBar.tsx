import {
  ArrowDownWideNarrow,
  Calendar,
  CalendarClock,
  ChartGantt,
  CircleDot,
  Contrast,
  Boxes,
  List,
  ListFilter,
  ListFilterPlus,
  Search,
  Sheet,
  SignalHigh,
  SlidersHorizontal,
  SquareKanban,
  Tag,
  UserRound,
  UserPen,
  X,
  Plus,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

import { PRIORITIES } from "../api/types";
import { useI18n } from "../i18n";
import { Popover } from "./Layout";
import { Avatar } from "./Avatar";
import { Dot } from "./ColorPicker";
import { Picker, type Option } from "./Picker";
import { PriorityIcon, StateIcon } from "./Props";
import type { PropsCtx } from "./WorkItemProps";
import { DEFAULT_VIEW, DISPLAY_KEYS, EMPTY_FILTERS, LAYOUTS, activeFilterCount, type DueFilter, type Filters, type GroupBy, type Layout, type OrderBy, type ViewState } from "../lib/view";

const LAYOUT_ICON: Record<Layout, ReactNode> = {
  list: <List />,
  kanban: <SquareKanban />,
  calendar: <Calendar />,
  spreadsheet: <Sheet />,
  gantt: <ChartGantt />,
};
// Plane's order: List, Board, Calendar, Table, Timeline.
const LAYOUT_ORDER: Layout[] = ["list", "kanban", "calendar", "spreadsheet", "gantt"];

/** The five layouts as one segmented control of icon tabs. */
export function LayoutSwitcher({ view, setView }: { view: ViewState; setView: (p: Partial<ViewState>) => void }) {
  const { t } = useI18n();
  return (
    <div className="seg" role="tablist" aria-label={t("view.layout")}>
      {LAYOUT_ORDER.filter((l) => LAYOUTS.includes(l)).map((l) => (
        <button key={l} role="tab" aria-selected={view.layout === l} aria-label={t(`view.${l}`)} title={t(`view.${l}`)} className={"seg-tab" + (view.layout === l ? " on" : "")} onClick={() => setView({ layout: l })}>
          {LAYOUT_ICON[l]}
        </button>
      ))}
    </div>
  );
}

/** Funnel button: opens the filter row; accent-tinted while filters apply. */
export function FiltersToggle({ open, onToggle, count }: { open: boolean; onToggle: () => void; count: number }) {
  const { t } = useI18n();
  const on = open || count > 0;
  return (
    <button className={"icon-btn bordered" + (on ? " accent" : "")} onClick={onToggle} aria-pressed={open} aria-label={count ? t("view.filtersN", { n: count }) : t("view.filters")} title={t("view.filters")}>
      {count ? <ListFilterPlus /> : <ListFilter />}
    </button>
  );
}

/** A search box that starts as an icon and opens on click. */
export function ExpandableSearch({ value, onChange, label }: { value: string; onChange: (v: string) => void; label: string }) {
  const [open, setOpen] = useState(!!value);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (open) ref.current?.focus();
  }, [open]);
  if (!open && !value) {
    return (
      <button className="icon-btn" onClick={() => setOpen(true)} aria-label={label} title={label}>
        <Search />
      </button>
    );
  }
  return (
    <span className="expand-search">
      <Search aria-hidden />
      <input
        ref={ref}
        placeholder={label}
        aria-label={label}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onBlur={() => !value && setOpen(false)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            onChange("");
            setOpen(false);
          }
        }}
      />
      {value ? (
        <button className="icon-btn icon-btn-xs" onClick={() => (onChange(""), setOpen(false))} aria-label={label}>
          <X />
        </button>
      ) : null}
    </span>
  );
}

type FilterKey = Exclude<keyof Filters, "q">;

/** Applied filters as Plane-style condition pills ("Priority · Urgent, High ×") under the
 * header, with "Add filter" and "Clear filters". */
export function FilterRow({ view, setView, ctx, extra }: { view: ViewState; setView: (p: Partial<ViewState>) => void; ctx: PropsCtx; extra?: ReactNode }) {
  const { t } = useI18n();
  const f = view.filters;
  const setF = (p: Partial<Filters>) => setView({ filters: { ...f, ...p } });
  const [adding, setAdding] = useState<FilterKey | null>(null);
  const [menu, setMenu] = useState(false);
  const closeMenu = useCallback(() => setMenu(false), []);
  const people: Option[] = ctx.members.map((m) => ({ value: m.user_id, label: m.name || m.email, icon: <Avatar name={m.name} email={m.email} src={m.avatar} size={18} /> }));
  const props: { key: FilterKey; label: string; icon: ReactNode; options: Option[]; single?: boolean; searchable?: boolean }[] = [
    { key: "priority", label: t("filter.priority"), icon: <SignalHigh />, options: PRIORITIES.map((p) => ({ value: p, label: t(`prio.${p}`), icon: <PriorityIcon p={p} /> })), searchable: false },
    { key: "state", label: t("filter.state"), icon: <CircleDot />, options: ctx.states.map((s) => ({ value: s.id, label: s.name, icon: <StateIcon group={s.group} color={s.color} /> })) },
    { key: "assignee", label: t("filter.assignee"), icon: <UserRound />, options: [{ value: "none", label: t("noAssignee") }, ...people] },
    { key: "label", label: t("filter.label"), icon: <Tag />, options: ctx.labels.map((l) => ({ value: l.id, label: l.name, icon: <Dot color={l.color} /> })) },
    ...(ctx.project.feature_cycles && ctx.cycles
      ? [{ key: "cycle" as const, label: t("filter.cycle"), icon: <Contrast />, options: [{ value: "none", label: t("noCycle") }, ...ctx.cycles.map((c) => ({ value: c.id, label: c.name, hint: t(`cyc.status.${c.status}`) }))] }]
      : []),
    ...(ctx.project.feature_modules && ctx.modules
      ? [{ key: "module" as const, label: t("filter.module"), icon: <Boxes />, options: [{ value: "none", label: t("noModule") }, ...ctx.modules.map((m) => ({ value: m.id, label: m.name }))] }]
      : []),
    { key: "due", label: t("filter.due"), icon: <CalendarClock />, options: [{ value: "overdue", label: t("due.overdue") }, { value: "week", label: t("due.week") }], single: true, searchable: false },
    { key: "created_by", label: t("filter.created_by"), icon: <UserPen />, options: people },
  ];
  const valueOf = (k: FilterKey): string[] => (k === "due" ? (f.due ? [f.due] : []) : (f[k] as string[]));
  const set = (k: FilterKey, v: string[]) => (k === "due" ? setF({ due: (v[0] ?? "") as DueFilter }) : setF({ [k]: v } as Partial<Filters>));
  const applied = props.filter((p) => valueOf(p.key).length > 0 || adding === p.key);
  const free = props.filter((p) => !applied.includes(p));
  const n = activeFilterCount(f);
  return (
    <div className="filter-row">
      <div className="filter-strip">
        {applied.map((p) => {
          const v = valueOf(p.key);
          const names = v.map((x) => p.options.find((o) => o.value === x)?.label ?? x);
          return (
            <span key={p.key} className="filter-pill">
              <span className="filter-pill-prop">
                {p.icon}
                {p.label}
              </span>
              <Picker
                label={t("view.filterBy", { name: p.label })}
                className="filter-pill-value"
                multi={!p.single}
                searchable={p.searchable ?? true}
                value={v}
                onChange={(nv) => set(p.key, nv)}
                options={p.options}
                autoOpen={adding === p.key}
                onClosed={() => setAdding(null)}
                trigger={<span className="ellipsis">{names.length ? names.join(", ") : t("view.pick")}</span>}
              />
              <button className="filter-pill-x" onClick={() => (set(p.key, []), setAdding(null))} aria-label={t("view.removeFilter", { name: p.label })}>
                <X />
              </button>
            </span>
          );
        })}
        {free.length ? (
          <Popover
            open={menu}
            onClose={closeMenu}
            trigger={
              <button className="btn btn-sm btn-ghost" onClick={() => setMenu(!menu)} aria-haspopup="menu" aria-expanded={menu}>
                <Plus />
                {t("view.addFilter")}
              </button>
            }
          >
            <div className="menu" role="menu">
              {free.map((p) => (
                <button key={p.key} className="menu-item" role="menuitem" onClick={() => (setMenu(false), setAdding(p.key))}>
                  {p.icon}
                  {p.label}
                </button>
              ))}
            </div>
          </Popover>
        ) : null}
        <span className="grow" />
        {extra ?? null}
        {n ? (
          <button className="btn btn-sm btn-ghost" onClick={() => setView({ filters: { ...EMPTY_FILTERS, q: f.q } })}>
            {t("view.clear")}
          </button>
        ) : null}
      </div>
    </div>
  );
}

/** Display: visible properties as toggle pills, then grouping, ordering and options. */
export function DisplayMenu({ view, setView, compact }: { view: ViewState; setView: (p: Partial<ViewState>) => void; compact?: boolean }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const changed =
    view.groupBy !== DEFAULT_VIEW.groupBy ||
    view.orderBy !== DEFAULT_VIEW.orderBy ||
    view.showSubItems !== DEFAULT_VIEW.showSubItems ||
    view.showEmptyGroups !== DEFAULT_VIEW.showEmptyGroups ||
    DISPLAY_KEYS.some((k) => view.display[k] !== DEFAULT_VIEW.display[k]);
  const grouped = view.layout === "list" || view.layout === "kanban";
  return (
    <Popover
      open={open}
      onClose={close}
      right
      trigger={
        <button className={compact ? "icon-btn bordered" : "btn"} onClick={() => setOpen((o) => !o)} aria-expanded={open} aria-label={t("view.display")} style={{ position: "relative" }}>
          <SlidersHorizontal />
          {compact ? null : t("view.display")}
          {changed ? <span className="dot-badge" aria-hidden /> : null}
        </button>
      }
    >
      <div className="menu display-menu">
        <div className="menu-label">{t("view.properties")}</div>
        <div className="toggle-chips">
          {DISPLAY_KEYS.map((k) => (
            <button key={k} type="button" className={"toggle-chip" + (view.display[k] ? " on" : "")} aria-pressed={view.display[k]} onClick={() => setView({ display: { ...view.display, [k]: !view.display[k] } })}>
              {t(`disp.${k}`)}
            </button>
          ))}
        </div>
        {grouped ? (
          <>
            <div className="menu-sep" />
            <label className="display-row">
              <span className="grow">{t("view.groupBy")}</span>
              <select className="select select-inline" value={view.groupBy} onChange={(e) => setView({ groupBy: e.target.value as GroupBy })}>
                {(["state", "priority", "assignee", "label", "none"] as GroupBy[]).map((g) => <option key={g} value={g}>{t(`groupBy.${g}`)}</option>)}
              </select>
            </label>
          </>
        ) : null}
        <label className="display-row">
          <ArrowDownWideNarrow aria-hidden className="faint" />
          <span className="grow">{t("view.orderBy")}</span>
          <select className="select select-inline" value={view.orderBy} onChange={(e) => setView({ orderBy: e.target.value as OrderBy })}>
            {(["manual", "priority", "due_date", "created_at", "updated_at"] as OrderBy[]).map((o) => <option key={o} value={o}>{t(`orderBy.${o}`)}</option>)}
          </select>
        </label>
        <div className="menu-sep" />
        <label className="menu-item checkbox-row"><input type="checkbox" checked={view.showSubItems} onChange={(e) => setView({ showSubItems: e.target.checked })} />{t("view.subItems")}</label>
        {grouped ? <label className="menu-item checkbox-row"><input type="checkbox" checked={view.showEmptyGroups} onChange={(e) => setView({ showEmptyGroups: e.target.checked })} />{t("view.emptyGroups")}</label> : null}
      </div>
    </Popover>
  );
}

