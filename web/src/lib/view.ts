// The view of a project's work items: layout, grouping, ordering, filters and which properties
// show. Pure functions here; the state is kept per project in localStorage (saved, shared views
// come in phase 2).
import { useCallback, useState } from "react";
import type { Label, Priority, ProjMember, WfState, WorkItem } from "../api/types";
import { PRIORITIES, STATE_GROUPS } from "../api/types";

export type Layout = "list" | "kanban" | "spreadsheet" | "calendar" | "gantt";
export const LAYOUTS: Layout[] = ["list", "kanban", "spreadsheet", "calendar", "gantt"];
export type GroupBy = "state" | "priority" | "assignee" | "label" | "none";
export type OrderBy = "manual" | "priority" | "due_date" | "created_at" | "updated_at";
export type DueFilter = "" | "overdue" | "week" | "none";

export interface Filters {
  state: string[];
  priority: Priority[];
  assignee: string[];
  label: string[];
  created_by: string[];
  cycle: string[];
  module: string[];
  due: DueFilter;
  q: string;
}

export const DISPLAY_KEYS = ["key", "priority", "assignees", "labels", "start_date", "due_date", "estimate", "sub_items"] as const;
export type DisplayKey = (typeof DISPLAY_KEYS)[number];

export interface ViewState {
  layout: Layout;
  groupBy: GroupBy;
  orderBy: OrderBy;
  showSubItems: boolean;
  showEmptyGroups: boolean;
  filters: Filters;
  display: Record<DisplayKey, boolean>;
}

export const EMPTY_FILTERS: Filters = { state: [], priority: [], assignee: [], label: [], created_by: [], cycle: [], module: [], due: "", q: "" };

export const DEFAULT_VIEW: ViewState = {
  layout: "list",
  groupBy: "state",
  orderBy: "manual",
  showSubItems: true,
  showEmptyGroups: true,
  filters: EMPTY_FILTERS,
  display: { key: true, priority: true, assignees: true, labels: true, start_date: false, due_date: true, estimate: true, sub_items: true },
};

export function activeFilterCount(f: Filters): number {
  return f.state.length + f.priority.length + f.assignee.length + f.label.length + f.created_by.length + f.cycle.length + f.module.length + (f.due ? 1 : 0);
}

const ymd = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** Server query string for the filters (the server does the filtering). */
export function filtersToQuery(f: Filters, today = new Date()): string {
  const p = new URLSearchParams();
  if (f.state.length) p.set("state", f.state.join(","));
  if (f.priority.length) p.set("priority", f.priority.join(","));
  const people = f.assignee.filter((a) => a !== "none");
  if (people.length) p.set("assignee", people.join(","));
  if (f.assignee.includes("none")) p.set("unassigned", "true");
  if (f.label.length) p.set("label", f.label.join(","));
  if (f.created_by.length) p.set("created_by", f.created_by.join(","));
  if (f.cycle.length) p.set("cycle", f.cycle.join(","));
  if (f.module.length) p.set("module", f.module.join(","));
  if (f.due === "overdue") {
    const y = new Date(today);
    y.setDate(y.getDate() - 1);
    p.set("due_before", ymd(y));
    p.set("state_group", "backlog,unstarted,started");
  } else if (f.due === "week") {
    const end = new Date(today);
    end.setDate(end.getDate() + 7);
    p.set("due_after", ymd(today));
    p.set("due_before", ymd(end));
  }
  if (f.q.trim()) p.set("q", f.q.trim());
  return p.toString();
}

const PRIO_RANK: Record<Priority, number> = { urgent: 0, high: 1, medium: 2, low: 3, none: 4 };

export function sortItems(items: WorkItem[], orderBy: OrderBy): WorkItem[] {
  const out = [...items];
  const byDate = (k: "due_date" | "created_at" | "updated_at", asc: boolean) => (a: WorkItem, b: WorkItem) => {
    const x = a[k] ?? "";
    const y = b[k] ?? "";
    if (!x && y) return 1;
    if (x && !y) return -1;
    return asc ? x.localeCompare(y) : y.localeCompare(x);
  };
  switch (orderBy) {
    case "manual":
      out.sort((a, b) => a.sort_order - b.sort_order || b.sequence - a.sequence);
      break;
    case "priority":
      out.sort((a, b) => PRIO_RANK[a.priority] - PRIO_RANK[b.priority] || a.sort_order - b.sort_order);
      break;
    case "due_date":
      out.sort(byDate("due_date", true));
      break;
    case "created_at":
      out.sort(byDate("created_at", false));
      break;
    case "updated_at":
      out.sort(byDate("updated_at", false));
      break;
  }
  return out;
}

export interface Group {
  /** State id, priority, user id, label id, "none" or "all". */
  key: string;
  kind: GroupBy;
  items: WorkItem[];
  state?: WfState;
  priority?: Priority;
  member?: ProjMember;
  label?: Label;
}

/** Groups items. An item with two labels (or assignees) appears in both groups, as in Plane. */
export function groupItems(
  items: WorkItem[],
  groupBy: GroupBy,
  ctx: { states: WfState[]; labels: Label[]; members: ProjMember[] },
  orderBy: OrderBy,
  showEmpty: boolean,
): Group[] {
  const sorted = sortItems(items, orderBy);
  let groups: Group[];
  switch (groupBy) {
    case "none":
      return [{ key: "all", kind: "none", items: sorted }];
    case "state": {
      const order = [...ctx.states].sort((a, b) => STATE_GROUPS.indexOf(a.group) - STATE_GROUPS.indexOf(b.group) || a.sort_order - b.sort_order);
      groups = order.map((s) => ({ key: s.id, kind: "state", state: s, items: sorted.filter((w) => w.state_id === s.id) }));
      break;
    }
    case "priority":
      groups = PRIORITIES.map((p) => ({ key: p, kind: "priority", priority: p, items: sorted.filter((w) => w.priority === p) }));
      break;
    case "assignee":
      groups = [
        ...ctx.members.map((m) => ({ key: m.user_id, kind: "assignee" as const, member: m, items: sorted.filter((w) => w.assignee_ids.includes(m.user_id)) })),
        { key: "none", kind: "assignee", items: sorted.filter((w) => w.assignee_ids.length === 0) },
      ];
      break;
    case "label":
      groups = [
        ...ctx.labels.map((l) => ({ key: l.id, kind: "label" as const, label: l, items: sorted.filter((w) => w.label_ids.includes(l.id)) })),
        { key: "none", kind: "label", items: sorted.filter((w) => w.label_ids.length === 0) },
      ];
      break;
  }
  return showEmpty ? groups : groups.filter((g) => g.items.length > 0);
}

/** The change that moves an item into group `g` (for drag and drop between groups). */
export function patchForGroup(g: Group, item: WorkItem): Partial<WorkItem> | null {
  switch (g.kind) {
    case "state":
      return item.state_id === g.key ? null : { state_id: g.key };
    case "priority":
      return item.priority === g.key ? null : { priority: g.key as Priority };
    case "assignee":
      if (g.key === "none") return item.assignee_ids.length ? { assignee_ids: [] } : null;
      return item.assignee_ids.includes(g.key) ? null : { assignee_ids: [g.key] };
    case "label":
      if (g.key === "none") return item.label_ids.length ? { label_ids: [] } : null;
      return item.label_ids.includes(g.key) ? null : { label_ids: [...item.label_ids, g.key] };
    default:
      return null;
  }
}

/** A sort key between two neighbours (either may be missing). */
export function sortBetween(prev: number | undefined, next: number | undefined): number {
  if (prev === undefined && next === undefined) return 65535;
  if (prev === undefined) return next! - 1000;
  if (next === undefined) return prev + 1000;
  return (prev + next) / 2;
}

function load(pid: string): ViewState {
  try {
    const raw = localStorage.getItem(`np.view.${pid}`);
    if (!raw) return DEFAULT_VIEW;
    const v = JSON.parse(raw) as Partial<ViewState>;
    return { ...DEFAULT_VIEW, ...v, filters: { ...EMPTY_FILTERS, ...v.filters }, display: { ...DEFAULT_VIEW.display, ...v.display } };
  } catch {
    return DEFAULT_VIEW;
  }
}

export function useViewState(pid: string): [ViewState, (patch: Partial<ViewState>) => void] {
  const [v, setV] = useState<ViewState>(() => load(pid));
  const set = useCallback(
    (patch: Partial<ViewState>) =>
      setV((cur) => {
        const next = { ...cur, ...patch };
        localStorage.setItem(`np.view.${pid}`, JSON.stringify({ ...next, filters: { ...next.filters, q: "" } }));
        return next;
      }),
    [pid],
  );
  return [v, set];
}

const DAY = 86_400_000;
export const parseDay = (d: string) => {
  const [y, m, dd] = d.split("-").map(Number);
  return Date.UTC(y!, m! - 1, dd!);
};
export const fmtDay = (ms: number) => new Date(ms).toISOString().slice(0, 10);
export const addDays = (d: string, n: number) => fmtDay(parseDay(d) + n * DAY);
export const daysBetween = (a: string, b: string) => Math.round((parseDay(b) - parseDay(a)) / DAY);

/** Weeks (Mon–Sun) covering the month of `ym` ("YYYY-MM"), as YYYY-MM-DD strings. */
export function monthGrid(ym: string): string[][] {
  const first = parseDay(`${ym}-01`);
  const dow = (new Date(first).getUTCDay() + 6) % 7;
  let d = first - dow * DAY;
  const weeks: string[][] = [];
  do {
    const w: string[] = [];
    for (let i = 0; i < 7; i++, d += DAY) w.push(fmtDay(d));
    weeks.push(w);
  } while (fmtDay(d).slice(0, 7) === ym);
  return weeks;
}
