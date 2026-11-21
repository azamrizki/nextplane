import { useId } from "react";
// Small visual pieces for work item properties, shared by lists, boards and the detail view.
import type { Priority, StateGroup, WfState } from "../api/types";

/** State-group glyphs (drawn on a 16-unit box): backlog = dotted ring, unstarted = ring,
 * started = ring with an inner ring, completed = disc with a check cut out, cancelled = disc
 * with a cross cut out. Colour = the state's own. */
export function StateIcon({ group, color, size = 14 }: { group: StateGroup; color: string; size?: number }) {
  const uid = useId();
  const common = { width: size, height: size, viewBox: "0 0 16 16", "aria-hidden": true, className: "state-icon" } as const;
  if (group === "backlog") {
    return (
      <svg {...common}>
        {Array.from({ length: 15 }, (_, i) => {
          const a = (i / 15) * 2 * Math.PI - Math.PI / 2;
          const x1 = 8 + Math.cos(a) * 5.6;
          const y1 = 8 + Math.sin(a) * 5.6;
          const x2 = 8 + Math.cos(a) * 6.5;
          const y2 = 8 + Math.sin(a) * 6.5;
          return <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} stroke={color} strokeWidth={1.3} strokeLinecap="round" />;
        })}
      </svg>
    );
  }
  if (group === "unstarted" || group === "started") {
    return (
      <svg {...common}>
        <circle cx={8} cy={8} r={6} fill="none" stroke={color} strokeWidth={1.5} />
        {group === "started" ? <circle cx={8} cy={8} r={2.6} fill="none" stroke={color} strokeWidth={1.5} /> : null}
      </svg>
    );
  }
  const id = `cut${uid.replace(/:/g, "")}`;
  return (
    <svg {...common}>
      <mask id={id}>
        <rect width={16} height={16} fill="#fff" />
        {group === "completed" ? (
          <path d="M5.2 8.2l1.9 1.9 3.7-3.9" stroke="#000" strokeWidth={1.6} fill="none" strokeLinecap="round" strokeLinejoin="round" />
        ) : (
          <path d="M5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4" stroke="#000" strokeWidth={1.6} strokeLinecap="round" />
        )}
      </mask>
      <circle cx={8} cy={8} r={7} fill={color} mask={`url(#${id})`} />
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

/** Priority glyphs: urgent = outlined square with "!", high/medium/low = three signal bars
 * with 3/2/1 lit, none = three unlit bars. */
export function PriorityIcon({ p, size = 14 }: { p: Priority; size?: number }) {
  if (p === "urgent") {
    return (
      <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden className="prio-icon">
        <rect x="1.75" y="1.75" width="12.5" height="12.5" rx="3" fill="none" stroke="var(--prio-urgent)" strokeWidth="1.5" />
        <path d="M8 4.8v3.8M8 11.1v.1" stroke="var(--prio-urgent)" strokeWidth="1.7" strokeLinecap="round" />
      </svg>
    );
  }
  const lit = { high: 3, medium: 2, low: 1, none: 0 }[p];
  const color = `var(--prio-${p})`;
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden className="prio-icon">
      {[0, 1, 2].map((i) => (
        <rect key={i} x={2.5 + i * 4} y={11 - i * 3.5} width="2.6" height={3 + i * 3.5} rx="1" fill={i < lit ? color : "var(--prio-unlit)"} />
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
