// Spreadsheet, calendar and timeline (Gantt) layouts.
import { ChevronLeft, ChevronRight } from "lucide-react";
import { DndContext, PointerSensor, useDraggable, useDroppable, useSensor, useSensors, type DragEndEvent } from "@dnd-kit/core";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useMemo, useRef, useState } from "react";
import type { WorkItem } from "../api/types";
import { useI18n } from "../i18n";
import { addDays, daysBetween, fmtDay, monthGrid, parseDay, sortItems, type ViewState } from "../lib/view";
import { keyOf } from "../lib/workItems";
import { PriorityIcon, StateIcon, fmtDate } from "./Props";
import { AssigneesProp, CycleProp, DateProp, EstimateProp, LabelsProp, ModulesProp, PriorityProp, StateProp, type PropsCtx } from "./WorkItemProps";

interface Props {
  items: WorkItem[];
  ctx: PropsCtx;
  view: ViewState;
  onOpen: (id: string) => void;
  activeId: string | null;
  onPatch: (id: string, body: Partial<WorkItem>) => void;
}

const today = () => fmtDay(Date.UTC(new Date().getFullYear(), new Date().getMonth(), new Date().getDate()));

// ---- Spreadsheet ------------------------------------------------------------------------

export function SpreadsheetLayout({ items, ctx, view, onOpen, activeId, onPatch }: Props) {
  const { t, lang } = useI18n();
  const rows = useMemo(() => sortItems(items, view.orderBy), [items, view.orderBy]);
  const ref = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: rows.length, getScrollElement: () => ref.current, estimateSize: () => 38, overscan: 15 });
  const cols = [
    ["key", t("col.key"), "80px"],
    ["title", t("col.title"), "360px"],
    ["state", t("wi.state"), "150px"],
    ["priority", t("wi.priority"), "130px"],
    ["assignees", t("wi.assignees"), "150px"],
    ["labels", t("wi.labels"), "160px"],
    ["start", t("wi.start"), "120px"],
    ["due", t("wi.due"), "120px"],
    ...(ctx.project.estimate_type !== "none" ? [["estimate", t("wi.estimate"), "100px"]] : []),
    ...(ctx.project.feature_cycles ? [["cycle", t("wi.cycle"), "140px"]] : []),
    ...(ctx.project.feature_modules ? [["modules", t("wi.modules"), "160px"]] : []),
    ["updated", t("col.updated"), "110px"],
  ] as [string, string, string][];
  const grid = { gridTemplateColumns: cols.map((c) => c[2]).join(" ") };
  const cell = (w: WorkItem, k: string) => {
    const p = (b: Partial<WorkItem>) => onPatch(w.id, b);
    switch (k) {
      case "key": return <span className="mono faint small">{keyOf(ctx.project.identifier, w.sequence)}</span>;
      case "title": return <button className="sheet-title ellipsis" onClick={() => onOpen(w.id)}>{w.title}</button>;
      case "state": return <StateProp item={w} ctx={ctx} onPatch={p} />;
      case "priority": return <PriorityProp item={w} ctx={ctx} onPatch={p} />;
      case "assignees": return <AssigneesProp item={w} ctx={ctx} onPatch={p} />;
      case "labels": return <LabelsProp item={w} ctx={ctx} onPatch={p} />;
      case "start": return <DateProp item={w} field="start_date" ctx={ctx} onPatch={p} />;
      case "due": return <DateProp item={w} field="due_date" ctx={ctx} onPatch={p} />;
      case "estimate": return <EstimateProp item={w} ctx={ctx} onPatch={p} />;
      case "cycle": return <CycleProp item={w} ctx={ctx} onPatch={p} />;
      case "modules": return <ModulesProp item={w} ctx={ctx} onPatch={p} />;
      case "updated": return <span className="faint small">{fmtDate(w.updated_at.slice(0, 10), lang)}</span>;
    }
    return null;
  };
  return (
    <div className="sheet" ref={ref} role="table" aria-rowcount={rows.length + 1}>
      <div className="sheet-row sheet-head" style={grid} role="row">
        {cols.map(([k, label]) => <div key={k} role="columnheader">{label}</div>)}
      </div>
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {v.getVirtualItems().map((vi) => {
          const w = rows[vi.index]!;
          return (
            <div key={w.id} className={"sheet-row" + (activeId === w.id ? " active" : "")} role="row" style={{ ...grid, position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${vi.start}px)` }} data-index={vi.index}>
              {cols.map(([k]) => <div key={k} role="cell" className="sheet-cell">{cell(w, k)}</div>)}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---- Calendar ---------------------------------------------------------------------------

export function CalendarLayout({ items, ctx, onOpen, onPatch }: Props) {
  const { t, lang } = useI18n();
  const [month, setMonth] = useState(today().slice(0, 7));
  const weeks = useMemo(() => monthGrid(month), [month]);
  const byDay = useMemo(() => {
    const m = new Map<string, WorkItem[]>();
    for (const w of items) if (w.due_date) m.set(w.due_date, [...(m.get(w.due_date) ?? []), w]);
    return m;
  }, [items]);
  const noDue = items.filter((w) => !w.due_date).length;
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 5 } }));
  const shift = (n: number) => {
    const [y, m] = month.split("-").map(Number);
    const d = new Date(Date.UTC(y!, m! - 1 + n, 1));
    setMonth(d.toISOString().slice(0, 7));
  };
  const onEnd = (e: DragEndEvent) => {
    if (!e.over || !ctx.editable) return;
    const w = items.find((x) => x.id === e.active.id);
    const day = String(e.over.id).replace("day:", "");
    if (!w || !w.due_date || w.due_date === day) return;
    const delta = daysBetween(w.due_date, day);
    // Moving the due date keeps the start date the same distance before it.
    onPatch(w.id, { due_date: day, ...(w.start_date ? { start_date: addDays(w.start_date, delta) } : {}) });
  };
  const label = new Date(parseDay(`${month}-01`)).toLocaleDateString(lang === "id" ? "id-ID" : "en-US", { month: "long", year: "numeric", timeZone: "UTC" });
  const dayNames = weeks[0]!.map((d) => new Date(parseDay(d)).toLocaleDateString(lang === "id" ? "id-ID" : "en-US", { weekday: "short", timeZone: "UTC" }));
  const now = today();
  return (
    <div className="cal">
      <div className="cal-bar">
        <button className="icon-btn" aria-label={t("cal.prev")} onClick={() => shift(-1)}><ChevronLeft /></button>
        <b className="cal-month">{label}</b>
        <button className="icon-btn" aria-label={t("cal.next")} onClick={() => shift(1)}><ChevronRight /></button>
        <button className="btn btn-sm" onClick={() => setMonth(now.slice(0, 7))}>{t("cal.today")}</button>
        <span className="grow" />
        {noDue ? <span className="faint small">{t("cal.noDue", { n: noDue })}</span> : null}
      </div>
      <DndContext sensors={sensors} onDragEnd={onEnd}>
        <div className="cal-grid" role="grid" aria-label={label}>
          {dayNames.map((d) => <div key={d} className="cal-dow" role="columnheader">{d}</div>)}
          {weeks.flat().map((d) => (
            <CalDay key={d} day={d} inMonth={d.slice(0, 7) === month} today={d === now} items={byDay.get(d) ?? []} ctx={ctx} onOpen={onOpen} />
          ))}
        </div>
      </DndContext>
    </div>
  );
}

function CalDay({ day, inMonth, today: isToday, items, ctx, onOpen }: { day: string; inMonth: boolean; today: boolean; items: WorkItem[]; ctx: PropsCtx; onOpen: (id: string) => void }) {
  const { setNodeRef, isOver } = useDroppable({ id: `day:${day}` });
  return (
    <div ref={setNodeRef} className={"cal-day" + (inMonth ? "" : " out") + (isToday ? " today" : "") + (isOver ? " over" : "")} role="gridcell" aria-label={day}>
      <span className="cal-num">{Number(day.slice(8))}</span>
      {items.map((w) => <CalItem key={w.id} w={w} ctx={ctx} onOpen={onOpen} />)}
    </div>
  );
}

function CalItem({ w, ctx, onOpen }: { w: WorkItem; ctx: PropsCtx; onOpen: (id: string) => void }) {
  const { attributes, listeners, setNodeRef, transform, isDragging } = useDraggable({ id: w.id, disabled: !ctx.editable });
  const s = ctx.states.find((x) => x.id === w.state_id);
  return (
    <button
      ref={setNodeRef}
      {...attributes}
      {...listeners}
      className="cal-item"
      style={{ transform: transform ? `translate(${transform.x}px, ${transform.y}px)` : undefined, opacity: isDragging ? 0.6 : 1, zIndex: isDragging ? 5 : undefined }}
      onClick={() => onOpen(w.id)}
      title={w.title}
    >
      {s ? <StateIcon group={s.group} color={s.color} size={12} /> : null}
      <PriorityIcon p={w.priority} size={12} />
      <span className="ellipsis">{w.title}</span>
    </button>
  );
}

// ---- Timeline (Gantt) -------------------------------------------------------------------

const DAY_W = 28;

export function GanttLayout({ items, ctx, view, onOpen, onPatch }: Props) {
  const { t, lang } = useI18n();
  const dated = useMemo(() => sortItems(items.filter((w) => w.start_date || w.due_date), view.orderBy), [items, view.orderBy]);
  const undated = items.filter((w) => !w.start_date && !w.due_date);
  const now = today();
  const [from, to] = useMemo(() => {
    let lo = addDays(now, -7);
    let hi = addDays(now, 28);
    for (const w of dated) {
      const s = w.start_date ?? w.due_date!;
      const e = w.due_date ?? w.start_date!;
      if (s < lo) lo = addDays(s, -3);
      if (e > hi) hi = addDays(e, 7);
    }
    return [lo, hi];
  }, [dated, now]);
  const days = daysBetween(from, to) + 1;
  const [drag, setDrag] = useState<{ id: string; mode: "move" | "start" | "end"; x0: number; dx: number } | null>(null);
  const x = (d: string) => daysBetween(from, d) * DAY_W;
  const onDown = (e: React.PointerEvent, id: string, mode: "move" | "start" | "end") => {
    if (!ctx.editable) return;
    e.preventDefault();
    e.stopPropagation();
    (e.target as Element).setPointerCapture(e.pointerId);
    setDrag({ id, mode, x0: e.clientX, dx: 0 });
  };
  const onMove = (e: React.PointerEvent) => drag && setDrag({ ...drag, dx: e.clientX - drag.x0 });
  const onUp = () => {
    if (!drag) return;
    const n = Math.round(drag.dx / DAY_W);
    const w = items.find((i) => i.id === drag.id);
    setDrag(null);
    if (!w || n === 0) {
      if (w && Math.abs(drag.dx) < 3) onOpen(w.id);
      return;
    }
    const s = w.start_date ?? w.due_date!;
    const en = w.due_date ?? w.start_date!;
    if (drag.mode === "move") onPatch(w.id, { start_date: addDays(s, n), due_date: addDays(en, n) });
    if (drag.mode === "start") onPatch(w.id, { start_date: addDays(s, n) <= en ? addDays(s, n) : en, due_date: en });
    if (drag.mode === "end") onPatch(w.id, { start_date: s, due_date: addDays(en, n) >= s ? addDays(en, n) : s });
  };
  const months: { label: string; left: number }[] = [];
  for (let i = 0; i < days; i++) {
    const d = addDays(from, i);
    if (i === 0 || d.endsWith("-01")) months.push({ label: new Date(parseDay(d)).toLocaleDateString(lang === "id" ? "id-ID" : "en-US", { month: "short", year: "numeric", timeZone: "UTC" }), left: i * DAY_W });
  }
  return (
    <div className="gantt" onPointerMove={onMove} onPointerUp={onUp}>
      <p className="faint small gantt-hint">{t("gantt.hint")}</p>
      <div className="gantt-scroll">
        <div className="gantt-names">
          <div className="gantt-corner" />
          {dated.map((w) => (
            <button key={w.id} className="gantt-name ellipsis" onClick={() => onOpen(w.id)}>
              <span className="mono faint small">{keyOf(ctx.project.identifier, w.sequence)}</span> {w.title}
            </button>
          ))}
        </div>
        <div className="gantt-chart" style={{ width: days * DAY_W }}>
          <div className="gantt-scale">
            {months.map((m) => <span key={m.left} className="gantt-month" style={{ left: m.left }}>{m.label}</span>)}
            {Array.from({ length: days }, (_, i) => {
              const d = addDays(from, i);
              const wd = new Date(parseDay(d)).getUTCDay();
              return <span key={d} className={"gantt-day" + (wd === 0 || wd === 6 ? " weekend" : "")} style={{ left: i * DAY_W }}>{Number(d.slice(8))}</span>;
            })}
          </div>
          <div className="gantt-today" style={{ left: x(now) + DAY_W / 2 }} aria-hidden />
          {dated.map((w) => {
            const s = w.start_date ?? w.due_date!;
            const e = w.due_date ?? w.start_date!;
            let left = x(s);
            let width = (daysBetween(s, e) + 1) * DAY_W;
            if (drag?.id === w.id) {
              if (drag.mode === "move") left += drag.dx;
              if (drag.mode === "start") { left += drag.dx; width -= drag.dx; }
              if (drag.mode === "end") width += drag.dx;
            }
            const st = ctx.states.find((z) => z.id === w.state_id);
            return (
              <div key={w.id} className="gantt-row">
                <div
                  className={"gantt-bar" + (width < 72 ? " narrow" : "")}
                  style={{ left, width: Math.max(DAY_W, width), borderColor: st?.color, background: `color-mix(in srgb, ${st?.color ?? "var(--accent)"} 18%, var(--bg))` }}
                  onPointerDown={(ev) => onDown(ev, w.id, "move")}
                  title={`${w.title} · ${fmtDate(s, lang)} – ${fmtDate(e, lang)}`}
                  role="button"
                  aria-label={`${w.title}: ${s} – ${e}`}
                >
                  <span className="gantt-handle l" onPointerDown={(ev) => onDown(ev, w.id, "start")} />
                  <span className="ellipsis small">{w.title}</span>
                  <span className="gantt-handle r" onPointerDown={(ev) => onDown(ev, w.id, "end")} />
                </div>
              </div>
            );
          })}
        </div>
      </div>
      {undated.length ? (
        <details className="gantt-undated">
          <summary className="small muted">{t("gantt.noDates")} ({undated.length})</summary>
          <div className="rows">
            {undated.map((w) => (
              <button key={w.id} className="row row-btn small" onClick={() => onOpen(w.id)}>
                <span className="mono faint">{keyOf(ctx.project.identifier, w.sequence)}</span> {w.title}
              </button>
            ))}
          </div>
        </details>
      ) : null}
    </div>
  );
}
