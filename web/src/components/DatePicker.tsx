// Our own date picker (replaces the browser's calendar): a frosted panel with a typed field,
// quick picks, a month grid with month and year views, and a footer that says what the date
// means. Three triggers use it: DateTrigger (any button: property chips, bulk actions),
// DateField (a form field) and DateRangeField (start and end, for cycles and modules).
import { ArrowRight, CalendarDays, CalendarRange, ChevronDown, ChevronLeft, ChevronRight, CircleAlert, CornerDownLeft, Repeat, X } from "lucide-react";
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useI18n, type T } from "../i18n";
import {
  addDays,
  addMonths,
  diffDays,
  formatLong,
  formatMonth,
  monthGrid,
  monthNames,
  monthOf,
  parseTyped,
  startOfWeek,
  todayIso,
  weekStartFor,
  weekdayNames,
  type Iso,
} from "../lib/dates";
import { Floating } from "./Floating";

export type DateKind = "due" | "start" | "plain";

/** What a date means, for the footer: "Due in 8 days", "Overdue by 4 days", "Starts tomorrow". */
export function relation(value: Iso | null, kind: DateKind, t: T, today = todayIso()): { text: string; tone: "" | "over" | "soon" } {
  if (!value) return { text: t("date.none"), tone: "" };
  const n = diffDays(today, value);
  if (kind === "due") {
    if (n === 0) return { text: t("date.dueToday"), tone: "soon" };
    if (n === 1) return { text: t("date.dueTomorrow"), tone: "" };
    if (n > 1) return { text: t("date.dueIn", { n }), tone: "" };
    return { text: n === -1 ? t("date.overdue1") : t("date.overdueN", { n: -n }), tone: "over" };
  }
  if (kind === "start") {
    if (n === 0) return { text: t("date.startsToday"), tone: "" };
    if (n === 1) return { text: t("date.startsTomorrow"), tone: "" };
    if (n > 1) return { text: t("date.startsIn", { n }), tone: "" };
    return { text: n === -1 ? t("date.started1") : t("date.startedN", { n: -n }), tone: "" };
  }
  if (n === 0) return { text: t("date.today"), tone: "" };
  if (n === 1) return { text: t("date.tomorrow"), tone: "" };
  if (n === -1) return { text: t("date.yesterday"), tone: "" };
  return { text: n > 0 ? t("date.inDays", { n }) : t("date.daysAgo", { n: -n }), tone: "" };
}

const daysLabel = (n: number, t: T) => (n === 1 ? t("date.day1") : t("date.days", { n }));
const inRange = (d: Iso, min?: string, max?: string) => (!min || d >= min) && (!max || d <= max);

/** Click outside closes a floating panel; its portal counts as inside. */
function useOutside(open: boolean, refs: React.RefObject<HTMLElement | null>[], close: () => void) {
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => {
      const n = e.target as Node;
      if (!refs.some((r) => r.current?.contains(n))) close();
    };
    document.addEventListener("mousedown", down);
    return () => document.removeEventListener("mousedown", down);
  }, [open]);
}

// ── The month grid (shared by the single and the range panel) ────────────────────────────────

function MonthGrid({
  month,
  focus,
  setFocus,
  onPick,
  onKey,
  dayClass,
  disabled,
  onHover,
  label,
}: {
  month: Iso;
  focus: Iso;
  setFocus: (d: Iso) => void;
  onPick: (d: Iso) => void;
  onKey: (e: React.KeyboardEvent) => void;
  dayClass: (d: Iso) => string;
  disabled: (d: Iso) => boolean;
  onHover?: (d: Iso | null) => void;
  label: string;
}) {
  const { lang } = useI18n();
  const ws = weekStartFor(lang);
  const cells = useMemo(() => monthGrid(month, ws), [month, ws]);
  const names = useMemo(() => weekdayNames(lang, ws), [lang, ws]);
  const today = todayIso();
  const rows = Array.from({ length: 6 }, (_, r) => cells.slice(r * 7, r * 7 + 7));
  return (
    <div className="dp-grid" role="grid" aria-label={label} onKeyDown={onKey} onMouseLeave={() => onHover?.(null)}>
      <div className="dp-row" role="row">
        {names.map((n) => (
          <span key={n} className="dp-wd" role="columnheader" aria-label={n}>
            {n}
          </span>
        ))}
      </div>
      {rows.map((row, r) => (
        <div className="dp-row" role="row" key={r}>
          {row.map((c, i) => {
            const off = disabled(c.iso);
            return (
              <span role="gridcell" key={c.iso} aria-selected={dayClass(c.iso).includes(" sel") || undefined}>
                <button
                  type="button"
                  data-iso={c.iso}
                  tabIndex={c.iso === focus ? 0 : -1}
                  disabled={off}
                  aria-label={formatLong(c.iso, lang, true)}
                  aria-current={c.iso === today ? "date" : undefined}
                  className={"dp-day" + (c.inMonth ? "" : " out") + (c.weekend ? " we" : "") + (c.iso === today ? " today" : "") + dayClass(c.iso) + (i === 0 ? " row-first" : "") + (i === 6 ? " row-last" : "")}
                  onClick={() => onPick(c.iso)}
                  onFocus={() => c.iso !== focus && setFocus(c.iso)}
                  onMouseEnter={() => onHover?.(c.iso)}
                >
                  {c.day}
                </button>
              </span>
            );
          })}
        </div>
      ))}
    </div>
  );
}

/** Arrow keys and friends move the focused day; returns the new day or null. */
function moveKey(e: React.KeyboardEvent, focus: Iso, lang: "en" | "id"): Iso | null {
  const ws = weekStartFor(lang);
  switch (e.key) {
    case "ArrowLeft":
      return addDays(focus, -1);
    case "ArrowRight":
      return addDays(focus, 1);
    case "ArrowUp":
      return addDays(focus, -7);
    case "ArrowDown":
      return addDays(focus, 7);
    case "PageUp":
      return addMonths(focus, e.shiftKey ? -12 : -1);
    case "PageDown":
      return addMonths(focus, e.shiftKey ? 12 : 1);
    case "Home":
      return startOfWeek(focus, ws);
    case "End":
      return addDays(startOfWeek(focus, ws), 6);
    case "t":
    case "T":
      return todayIso();
  }
  return null;
}

/** Month and year views behind the title. */
function MonthYearPicker({ month, onMonth, onBack }: { month: Iso; onMonth: (m: Iso) => void; onBack: () => void }) {
  const { t, lang } = useI18n();
  const [years, setYears] = useState(false);
  const [year, setYear] = useState(+month.slice(0, 4));
  const now = todayIso();
  const names = monthNames(lang, "short");
  const block = Math.floor(year / 12) * 12;
  return (
    <div className="dp-my">
      <div className="dp-head">
        <button type="button" className="dp-title" onClick={() => setYears((y) => !y)} aria-label={years ? t("date.chooseMonth") : t("date.chooseYear")}>
          {years ? `${block} – ${block + 11}` : year}
          {years ? null : <ChevronDown aria-hidden />}
        </button>
        <span className="dp-nav">
          <button type="button" className="icon-btn icon-btn-sm" aria-label={years ? t("date.prevYears") : t("date.prevYear")} onClick={() => setYear((y) => y - (years ? 12 : 1))}>
            <ChevronLeft />
          </button>
          <button type="button" className="icon-btn icon-btn-sm" aria-label={years ? t("date.nextYears") : t("date.nextYear")} onClick={() => setYear((y) => y + (years ? 12 : 1))}>
            <ChevronRight />
          </button>
        </span>
      </div>
      <div className="dp-mgrid">
        {years
          ? Array.from({ length: 12 }, (_, i) => block + i).map((y) => (
              <button type="button" key={y} className={"dp-m" + (y === +month.slice(0, 4) ? " sel" : "") + (y === +now.slice(0, 4) ? " cur" : "")} onClick={() => (setYear(y), setYears(false))}>
                {y}
              </button>
            ))
          : names.map((n, i) => {
              const m = `${year}-${String(i + 1).padStart(2, "0")}-01`;
              return (
                <button type="button" key={n} className={"dp-m" + (m === monthOf(month) ? " sel" : "") + (m === monthOf(now) ? " cur" : "")} onClick={() => onMonth(m)}>
                  {n}
                </button>
              );
            })}
      </div>
      <div className="dp-foot">
        <span className="dp-rel">
          <CornerDownLeft aria-hidden />
          {years ? t("date.chooseYear") : t("date.chooseMonth")}
        </span>
        <button type="button" className="btn btn-sm btn-ghost" onClick={onBack}>
          {t("date.back")}
        </button>
      </div>
    </div>
  );
}

// ── Single date ──────────────────────────────────────────────────────────────────────────────

export function DatePanel({
  value,
  onChange,
  onDone,
  kind = "plain",
  min,
  max,
  clearable = true,
  clearAlways,
  completed,
}: {
  value: Iso | null;
  onChange: (v: Iso | null) => void;
  onDone: () => void;
  kind?: DateKind;
  min?: string;
  max?: string;
  clearable?: boolean;
  /** Offer "Clear" even with no date (the bulk bar clears a date on many items). */
  clearAlways?: boolean;
  completed?: boolean;
}) {
  const { t, lang } = useI18n();
  const today = todayIso();
  const [focus, setFocus] = useState<Iso>(value ?? (min && today < min ? min : today));
  const [month, setMonth] = useState<Iso>(monthOf(focus));
  const [view, setView] = useState<"days" | "months">("days");
  const [text, setText] = useState(value ? formatLong(value, lang) : "");
  const [typing, setTyping] = useState(false);
  const grid = useRef<HTMLDivElement>(null);
  const moved = useRef(false);
  const parsed = typing ? parseTyped(text, lang, today) : value;
  const okParsed = parsed && inRange(parsed, min, max) ? parsed : null;

  const pick = (d: Iso | null) => {
    if (d && !inRange(d, min, max)) return;
    onChange(d);
    onDone();
  };
  useLayoutEffect(() => {
    if (!moved.current) return;
    moved.current = false;
    grid.current?.querySelector<HTMLButtonElement>(`[data-iso="${focus}"]`)?.focus();
  }, [focus, month]);
  const go = (d: Iso) => {
    moved.current = true;
    setFocus(d);
    if (monthOf(d) !== month) setMonth(monthOf(d));
  };
  const onGridKey = (e: React.KeyboardEvent) => {
    const next = moveKey(e, focus, lang);
    if (next) {
      e.preventDefault();
      go(next);
    }
  };
  const presets: [string, Iso][] = [
    [t("date.today"), today],
    [t("date.tomorrow"), addDays(today, 1)],
    [t("date.nextWeek"), addDays(today, 7)],
    [t("date.in2Weeks"), addDays(today, 14)],
  ];
  const rel = relation(okParsed, completed ? "plain" : kind, t, today);
  return (
    <div
      className="dp"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onDone();
        }
      }}
    >
      {view === "months" ? (
        <MonthYearPicker
          month={month}
          onBack={() => setView("days")}
          onMonth={(m) => {
            setMonth(m);
            setView("days");
          }}
        />
      ) : (
        <>
          <div className={"input dp-input" + (typing && text.trim() && !okParsed ? " invalid" : "")}>
            <CalendarDays aria-hidden />
            <input
              autoFocus
              aria-label={t("date.type")}
              placeholder={t("date.type")}
              value={text}
              onFocus={(e) => e.target.select()}
              onChange={(e) => {
                setText(e.target.value);
                setTyping(true);
                const d = parseTyped(e.target.value, lang, today);
                if (d) {
                  setFocus(d);
                  setMonth(monthOf(d));
                }
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  if (!text.trim() && clearable) pick(null);
                  else if (okParsed) pick(okParsed);
                } else if (e.key === "ArrowDown") {
                  e.preventDefault();
                  go(focus);
                }
              }}
            />
            {typing && text.trim() ? <span className={"dp-resolved" + (okParsed ? "" : " bad")}>{okParsed ? formatLong(okParsed, lang, true) : t("date.unknown")}</span> : null}
          </div>
          <div className="dp-presets">
            {presets.map(([label, d]) => (
              <button type="button" key={label} className={"dp-preset" + (value === d ? " on" : "")} disabled={!inRange(d, min, max)} onClick={() => pick(d)}>
                {label}
              </button>
            ))}
          </div>
          <div className="dp-head">
            <button type="button" className="dp-title" onClick={() => setView("months")} aria-label={t("date.chooseMonth")}>
              {formatMonth(month, lang)}
              <ChevronDown aria-hidden />
            </button>
            <span className="dp-nav">
              <button type="button" className="icon-btn icon-btn-sm" aria-label={t("date.prevMonth")} onClick={() => setMonth(addMonths(month, -1))}>
                <ChevronLeft />
              </button>
              <button type="button" className="icon-btn icon-btn-sm" aria-label={t("date.nextMonth")} onClick={() => setMonth(addMonths(month, 1))}>
                <ChevronRight />
              </button>
            </span>
          </div>
          <div ref={grid}>
            <MonthGrid
              month={month}
              focus={monthOf(focus) === month ? focus : month}
              setFocus={setFocus}
              onPick={pick}
              onKey={(e) => {
                if (e.key === "Enter" || e.key === " ") return; // the button's own click
                onGridKey(e);
              }}
              dayClass={(d) => (d === value ? " sel" : "") + (kind === "due" && !completed && d === value && d < today ? " over" : "")}
              disabled={(d) => !inRange(d, min, max)}
              label={formatMonth(month, lang)}
            />
          </div>
          <div className="dp-foot">
            <span className={"dp-rel" + (rel.tone ? ` ${rel.tone}` : "")}>
              {rel.tone === "over" ? <CircleAlert aria-hidden /> : <CalendarDays aria-hidden />}
              {rel.text}
            </span>
            {clearable && (value || clearAlways) ? (
              <button type="button" className="btn btn-sm btn-ghost" onClick={() => pick(null)}>
                {t("date.clear")}
              </button>
            ) : null}
          </div>
        </>
      )}
    </div>
  );
}

/** Any button that opens the date panel (property chips, the bulk bar, form fields). */
export function DateTrigger({
  value,
  onChange,
  kind = "plain",
  min,
  max,
  clearable = true,
  clearAlways,
  completed,
  className,
  children,
  id,
  title,
  disabled,
  align = "start",
  "aria-label": ariaLabel,
  "aria-describedby": describedBy,
}: {
  value: Iso | null;
  onChange: (v: Iso | null) => void;
  kind?: DateKind;
  min?: string;
  max?: string;
  clearable?: boolean;
  clearAlways?: boolean;
  completed?: boolean;
  className?: string;
  children: ReactNode;
  id?: string;
  title?: string;
  disabled?: boolean;
  align?: "start" | "end";
  "aria-label"?: string;
  "aria-describedby"?: string;
}) {
  const [open, setOpen] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const close = (refocus = true) => {
    setOpen(false);
    if (refocus) btn.current?.focus();
  };
  useOutside(open, [btn, pop], () => close(false));
  return (
    <>
      <button
        ref={btn}
        type="button"
        id={id}
        className={className}
        title={title}
        disabled={disabled}
        aria-label={ariaLabel}
        aria-describedby={describedBy}
        aria-haspopup="dialog"
        aria-expanded={open}
        data-value={value ?? ""}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((o) => !o);
        }}
      >
        {children}
      </button>
      {open ? (
        <Floating ref={pop} anchor={btn} align={align} className="popover dp-pop" onMouseDown={(e) => e.stopPropagation()} onClick={(e) => e.stopPropagation()}>
          <div className="dp-panel" role="dialog" aria-label={ariaLabel}>
            <DatePanel value={value} onChange={onChange} onDone={close} kind={kind} min={min} max={max} clearable={clearable} clearAlways={clearAlways} completed={completed} />
          </div>
        </Floating>
      ) : null}
    </>
  );
}

/** A form field: looks like an input, shows the date and what it means, opens the panel. */
export function DateField({
  id,
  value,
  onChange,
  placeholder,
  kind = "plain",
  min,
  max,
  required,
  clearable = !required,
  disabled,
  "aria-label": ariaLabel,
}: {
  id?: string;
  value: Iso | null;
  onChange: (v: Iso | null) => void;
  placeholder?: string;
  kind?: DateKind;
  min?: string;
  max?: string;
  required?: boolean;
  clearable?: boolean;
  disabled?: boolean;
  "aria-label"?: string;
}) {
  const { t, lang } = useI18n();
  return (
    <span className="date-field-wrap">
      <DateTrigger id={id} value={value} onChange={onChange} kind={kind} min={min} max={max} clearable={clearable} disabled={disabled} aria-label={ariaLabel} className="select date-field">
        <CalendarDays aria-hidden />
        <span className={"select-value" + (value ? "" : " placeholder")}>{value ? formatLong(value, lang) : placeholder ?? t("date.type")}</span>
        {value ? <span className="date-field-rel">{relation(value, kind, t).text}</span> : null}
      </DateTrigger>
      {required ? <input className="select-native" tabIndex={-1} aria-hidden required value={value ?? ""} onChange={() => undefined} /> : null}
    </span>
  );
}

// ── Range (cycles, modules) ──────────────────────────────────────────────────────────────────

export interface Range {
  start: Iso | null;
  end: Iso | null;
}
export interface NamedRange {
  name: string;
  start: Iso;
  end: Iso;
}

function RangePanel({
  value,
  onApply,
  onDone,
  conflicts = [],
  last,
  first,
}: {
  value: Range;
  onApply: (r: Range) => void;
  onDone: () => void;
  conflicts?: NamedRange[];
  last?: NamedRange | null;
  first: "start" | "end";
}) {
  const { t, lang } = useI18n();
  const today = todayIso();
  const ws = weekStartFor(lang);
  const [r, setR] = useState<Range>(value);
  const [side, setSide] = useState<"start" | "end">(first === "end" && value.start ? "end" : "start");
  const [hover, setHover] = useState<Iso | null>(null);
  const [month, setMonth] = useState<Iso>(monthOf((first === "end" ? value.end : value.start) ?? value.start ?? today));
  const [focus, setFocus] = useState<Iso>(value.start ?? today);
  const [text, setText] = useState({ start: value.start ? formatLong(value.start, lang) : "", end: value.end ? formatLong(value.end, lang) : "" });
  const moved = useRef(false);
  const months = useRef<HTMLDivElement>(null);
  const second = addMonths(month, 1);

  useLayoutEffect(() => {
    if (!moved.current) return;
    moved.current = false;
    months.current?.querySelector<HTMLButtonElement>(`[data-iso="${focus}"]`)?.focus();
  }, [focus, month]);

  const set = (next: Range, nextSide: "start" | "end") => {
    setR(next);
    setSide(nextSide);
    setText({ start: next.start ? formatLong(next.start, lang) : "", end: next.end ? formatLong(next.end, lang) : "" });
  };
  const pick = (d: Iso) => {
    if (side === "start" || !r.start) set({ start: d, end: r.end && r.end >= d ? r.end : null }, "end");
    else if (d < r.start) set({ start: d, end: null }, "end");
    else set({ start: r.start, end: d }, "start");
  };
  const typed = (which: "start" | "end", s: string) => {
    const d = parseTyped(s, lang, today);
    if (!d) return;
    if (which === "start") set({ start: d, end: r.end && r.end >= d ? r.end : null }, "end");
    else if (!r.start || d >= r.start) set({ start: r.start, end: d }, "start");
    setMonth(monthOf(which === "end" && r.start ? r.start : d));
  };
  const lo = r.start;
  const hi = r.end ?? (side === "end" && r.start && hover && hover >= r.start ? hover : null);
  const dayClass = (d: Iso) => {
    let c = "";
    if (lo && d === lo) c += " rs sel";
    if (hi && d === hi) c += " re sel";
    if (lo && hi && d > lo && d < hi) c += " in";
    if (!r.end && hi && d === hi) c += " preview";
    return c;
  };
  const len = r.start && r.end ? diffDays(r.start, r.end) + 1 : null;
  const overlap = r.start && r.end ? conflicts.find((c) => c.start <= r.end! && c.end >= r.start!) : undefined;
  const wk = startOfWeek(today, ws);
  const quick: [string, Range, ReactNode?][] = [
    [t("date.thisWeek"), { start: wk, end: addDays(wk, 6) }],
    [t("date.nextWeek"), { start: addDays(wk, 7), end: addDays(wk, 13) }],
    [t("date.next2Weeks"), { start: addDays(wk, 7), end: addDays(wk, 20) }],
    [t("date.thisMonth"), { start: monthOf(today), end: addDays(addMonths(monthOf(today), 1), -1) }],
    [t("date.nextMonthRange"), { start: addMonths(monthOf(today), 1), end: addDays(addMonths(monthOf(today), 2), -1) }],
  ];
  if (last) {
    const n = diffDays(last.start, last.end) + 1;
    const s = addDays(last.end, 1);
    quick.push([t("date.likeLast"), { start: s, end: addDays(s, n - 1) }, t("date.likeLastSub", { n, date: formatLong(s, lang) })]);
  }
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") return;
    const next = moveKey(e, focus, lang);
    if (!next) return;
    e.preventDefault();
    moved.current = true;
    setFocus(next);
    if (next < month) setMonth(monthOf(next));
    else if (next >= addMonths(month, 2)) setMonth(addMonths(monthOf(next), -1));
  };
  const field = (which: "start" | "end") => (
    <div className={"input dp-input" + (side === which ? " focus" : "")}>
      <span className="dp-side">{which === "start" ? t("date.start") : t("date.end")}</span>
      <input
        aria-label={which === "start" ? t("date.start") : t("date.end")}
        value={text[which]}
        autoFocus={side === which}
        onFocus={(e) => (setSide(which), e.target.select())}
        onChange={(e) => setText({ ...text, [which]: e.target.value })}
        onBlur={(e) => typed(which, e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            typed(which, (e.target as HTMLInputElement).value);
          }
        }}
      />
    </div>
  );
  return (
    <div
      className="drp"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onDone();
        }
      }}
    >
      <div className="drp-side">
        <div className="menu-label">{t("date.quick")}</div>
        {quick.map(([label, q, sub]) => {
          const on = q.start === r.start && q.end === r.end;
          return (
            <button
              type="button"
              key={label}
              className={"menu-item" + (sub ? " menu-item-2line" : "") + (on ? " active" : "")}
              onClick={() => {
                set(q, "start");
                setMonth(monthOf(q.start!));
              }}
            >
              {sub ? <Repeat aria-hidden /> : null}
              {sub ? (
                <span className="menu-item-text">
                  <span>{label}</span>
                  <span className="menu-item-sub">{sub}</span>
                </span>
              ) : (
                <span className="grow">{label}</span>
              )}
            </button>
          );
        })}
      </div>
      <div className="drp-main">
        <div className="drp-top">
          {field("start")}
          <ArrowRight className="faint" aria-hidden />
          {field("end")}
          {len ? <span className="badge">{daysLabel(len, t)}</span> : null}
        </div>
        <div className="drp-months" ref={months}>
          {[month, second].map((m, i) => (
            <div key={m}>
              <div className="dp-head">
                {i === 0 ? (
                  <button type="button" className="icon-btn icon-btn-sm" aria-label={t("date.prevMonth")} onClick={() => setMonth(addMonths(month, -1))}>
                    <ChevronLeft />
                  </button>
                ) : (
                  <span className="dp-nav-gap" />
                )}
                <span className="dp-title static">{formatMonth(m, lang)}</span>
                {i === 1 ? (
                  <button type="button" className="icon-btn icon-btn-sm" aria-label={t("date.nextMonth")} onClick={() => setMonth(addMonths(month, 1))}>
                    <ChevronRight />
                  </button>
                ) : (
                  <span className="dp-nav-gap" />
                )}
              </div>
              <MonthGrid
                month={m}
                focus={monthOf(focus) === m ? focus : i === 0 && monthOf(focus) !== second ? m : ""}
                setFocus={setFocus}
                onPick={pick}
                onKey={onKey}
                onHover={setHover}
                dayClass={(d) => (monthOf(d) === m ? dayClass(d) : "")}
                disabled={() => false}
                label={formatMonth(m, lang)}
              />
            </div>
          ))}
        </div>
        <div className="drp-foot">
          {overlap ? (
            <span className="dp-warn">
              <CircleAlert aria-hidden />
              {t("date.overlaps", { name: overlap.name, range: `${formatLong(overlap.start, lang)} – ${formatLong(overlap.end, lang)}` })}
            </span>
          ) : (
            <span className="dp-rel">
              <CalendarRange aria-hidden />
              {side === "start" || !r.start ? t("date.pickStart") : r.end ? relation(r.start, "start", t, today).text : t("date.pickEnd")}
            </span>
          )}
          <span className="grow" />
          {value.start || value.end ? (
            <button type="button" className="btn btn-sm btn-ghost" onClick={() => (onApply({ start: null, end: null }), onDone())}>
              {t("date.clear")}
            </button>
          ) : null}
          <button type="button" className="btn btn-sm btn-ghost" onClick={onDone}>
            {t("common.cancel")}
          </button>
          <button type="button" className="btn btn-sm btn-primary" disabled={!r.start || !r.end} onClick={() => (onApply(r), onDone())}>
            {t("date.apply")}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Start and end in one field: "Nov 23, 2026 → Dec 6, 2026 · 14 days"; either half opens the
 * range panel. Each half is labelled for screen readers (e.g. "Start date", "End date"). */
export function DateRangeField({
  value,
  onChange,
  startLabel,
  endLabel,
  conflicts,
  last,
  disabled,
  "aria-labelledby": labelledBy,
}: {
  value: Range;
  onChange: (r: Range) => void;
  startLabel: string;
  endLabel: string;
  conflicts?: NamedRange[];
  last?: NamedRange | null;
  disabled?: boolean;
  "aria-labelledby"?: string;
}) {
  const { t, lang } = useI18n();
  const [open, setOpen] = useState<null | "start" | "end">(null);
  const box = useRef<HTMLDivElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const id = useId();
  const close = () => {
    setOpen(null);
    box.current?.querySelector<HTMLButtonElement>("button")?.focus();
  };
  useOutside(!!open, [box, pop], () => setOpen(null));
  const len = value.start && value.end ? diffDays(value.start, value.end) + 1 : null;
  const part = (which: "start" | "end") => {
    const v = value[which];
    return (
      <button
        type="button"
        className={"range-part" + (open === which ? " on" : "")}
        aria-label={which === "start" ? startLabel : endLabel}
        aria-haspopup="dialog"
        aria-expanded={open === which}
        aria-describedby={len ? `${id}-len` : undefined}
        data-value={v ?? ""}
        disabled={disabled}
        onClick={() => setOpen((o) => (o === which ? null : which))}
      >
        {which === "start" ? <CalendarDays aria-hidden /> : null}
        <span className={v ? "" : "placeholder"}>{v ? formatLong(v, lang) : which === "start" ? startLabel : endLabel}</span>
      </button>
    );
  };
  return (
    <div ref={box} className={"select range-field" + (open ? " open" : "")} role="group" aria-labelledby={labelledBy}>
      {part("start")}
      <ArrowRight className="range-arrow" aria-hidden />
      {part("end")}
      {len ? (
        <span className="range-len" id={`${id}-len`}>
          {daysLabel(len, t)}
        </span>
      ) : null}
      {value.start || value.end ? (
        <button type="button" className="icon-btn icon-btn-xs range-clear" aria-label={t("date.clear")} disabled={disabled} onClick={() => onChange({ start: null, end: null })}>
          <X />
        </button>
      ) : null}
      {open ? (
        <Floating ref={pop} anchor={box} className="popover dp-pop" onMouseDown={(e) => e.stopPropagation()} onClick={(e) => e.stopPropagation()}>
          <div className="dp-panel" role="dialog" aria-label={t("date.dates")}>
            <RangePanel value={value} onApply={onChange} onDone={close} conflicts={conflicts} last={last} first={open} />
          </div>
        </Floating>
      ) : null}
    </div>
  );
}


