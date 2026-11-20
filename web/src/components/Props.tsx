// Small visual pieces for work item properties, shared by lists, boards and the detail view.
import type { Priority, StateGroup, WfState } from "../api/types";

export function StateIcon({ group, color, size = 14 }: { group: StateGroup; color: string; size?: number }) {
  const r = size / 2 - 1.5;
  const c = size / 2;
  const circ = 2 * Math.PI * (r - 2);
  const fill = { backlog: 0, unstarted: 0, started: 0.5, completed: 1, cancelled: 1 }[group];
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden className="state-icon">
      <circle cx={c} cy={c} r={r} fill="none" stroke={color} strokeWidth={1.5} strokeDasharray={group === "backlog" ? "2 2" : undefined} />
      {group === "started" ? (
        <circle cx={c} cy={c} r={r - 2} fill="none" stroke={color} strokeWidth={(r - 2) * 2} strokeDasharray={`${circ * fill} ${circ}`} transform={`rotate(-90 ${c} ${c})`} />
      ) : null}
      {group === "completed" ? (
        <>
          <circle cx={c} cy={c} r={r} fill={color} />
          <path d={`M${c - 2.5} ${c} l1.8 1.8 l3.4 -3.6`} stroke="#fff" strokeWidth={1.5} fill="none" strokeLinecap="round" strokeLinejoin="round" />
        </>
      ) : null}
      {group === "cancelled" ? (
        <>
          <circle cx={c} cy={c} r={r} fill={color} />
          <path d={`M${c - 2} ${c - 2} l4 4 M${c + 2} ${c - 2} l-4 4`} stroke="#fff" strokeWidth={1.5} strokeLinecap="round" />
        </>
      ) : null}
    </svg>
  );
}

export function StateBadge({ state }: { state: WfState | undefined }) {
  if (!state) return null;
  return (
    <span className="prop-inline">
      <StateIcon group={state.group} color={state.color} />
      <span>{state.name}</span>
    </span>
  );
}

export function PriorityIcon({ p, size = 14 }: { p: Priority; size?: number }) {
  if (p === "urgent") {
    return (
      <svg width={size} height={size} viewBox="0 0 14 14" aria-hidden>
        <rect x="1" y="1" width="12" height="12" rx="3" fill="var(--prio-urgent)" />
        <path d="M7 3.8v4M7 10v.2" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" />
      </svg>
    );
  }
  if (p === "none") {
    return (
      <svg width={size} height={size} viewBox="0 0 14 14" aria-hidden>
        <path d="M2.5 7h2M6 7h2M9.5 7h2" stroke="var(--prio-none)" strokeWidth="1.6" strokeLinecap="round" />
      </svg>
    );
  }
  const n = { high: 3, medium: 2, low: 1 }[p];
  const color = `var(--prio-${p})`;
  return (
    <svg width={size} height={size} viewBox="0 0 14 14" aria-hidden>
      {[0, 1, 2].map((i) => (
        <rect key={i} x={2 + i * 4} y={9 - i * 3} width="2.6" height={3 + i * 3} rx="0.8" fill={i < n ? color : "var(--border-strong)"} />
      ))}
    </svg>
  );
}

/** "Nov 20" or "Nov 20, 2027" (other years); dates are YYYY-MM-DD. */
export function fmtDate(d: string, lang = "en"): string {
  const [y, m, day] = d.split("-").map(Number);
  const date = new Date(y!, m! - 1, day!);
  const sameYear = date.getFullYear() === new Date().getFullYear();
  return date.toLocaleDateString(lang === "id" ? "id-ID" : "en-US", { month: "short", day: "numeric", ...(sameYear ? {} : { year: "numeric" }) });
}

export function isOverdue(due: string | null, completed: string | null): boolean {
  if (!due || completed) return false;
  const today = new Date();
  const t = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;
  return due < t;
}
