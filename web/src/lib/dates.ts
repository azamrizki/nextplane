// Calendar days as "YYYY-MM-DD" strings in local time (how the API stores start, due and cycle
// dates), the month grid for the date picker, and the parser behind its typed field.
import type { Lang } from "../i18n";

export type Iso = string;

const pad = (n: number) => String(n).padStart(2, "0");

export function toIso(d: Date): Iso {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function fromIso(s: string | null | undefined): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s ?? "");
  if (!m) return null;
  const d = new Date(+m[1]!, +m[2]! - 1, +m[3]!);
  return toIso(d) === s ? d : null;
}

export const todayIso = (now = new Date()): Iso => toIso(now);

export function addDays(iso: Iso, n: number): Iso {
  const d = fromIso(iso)!;
  d.setDate(d.getDate() + n);
  return toIso(d);
}

/** Same day n months later, clamped to the month's last day (Jan 31 + 1 month = Feb 28). */
export function addMonths(iso: Iso, n: number): Iso {
  const d = fromIso(iso)!;
  const day = d.getDate();
  d.setDate(1);
  d.setMonth(d.getMonth() + n);
  const last = new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate();
  d.setDate(Math.min(day, last));
  return toIso(d);
}

/** Whole days from a to b (b − a). */
export function diffDays(a: Iso, b: Iso): number {
  const x = fromIso(a)!;
  const y = fromIso(b)!;
  return Math.round((Date.UTC(y.getFullYear(), y.getMonth(), y.getDate()) - Date.UTC(x.getFullYear(), x.getMonth(), x.getDate())) / 86_400_000);
}

/** The week starts on the language's first day: Sunday in English (US), Monday in Indonesian. */
export const weekStartFor = (lang: Lang): 0 | 1 => (lang === "id" ? 1 : 0);

export function startOfWeek(iso: Iso, weekStart: 0 | 1): Iso {
  const d = fromIso(iso)!;
  return addDays(iso, -((d.getDay() - weekStart + 7) % 7));
}

export const monthOf = (iso: Iso): Iso => iso.slice(0, 8) + "01";

export interface DayCell {
  iso: Iso;
  day: number;
  inMonth: boolean;
  weekend: boolean;
}

/** Six weeks covering the month of `month` (any day of it). */
export function monthGrid(month: Iso, weekStart: 0 | 1): DayCell[] {
  const first = monthOf(month);
  const m = first.slice(5, 7);
  let cur = startOfWeek(first, weekStart);
  const out: DayCell[] = [];
  for (let i = 0; i < 42; i++) {
    const d = fromIso(cur)!;
    out.push({ iso: cur, day: d.getDate(), inMonth: cur.slice(5, 7) === m, weekend: d.getDay() === 0 || d.getDay() === 6 });
    cur = addDays(cur, 1);
  }
  return out;
}

const locale = (lang: Lang) => (lang === "id" ? "id-ID" : "en-US");

/** Short weekday names in grid order ("Su Mo …" / "Sen Sel …"). */
export function weekdayNames(lang: Lang, weekStart: 0 | 1): string[] {
  const f = new Intl.DateTimeFormat(locale(lang), { weekday: "short" });
  // 2026-11-01 is a Sunday.
  return Array.from({ length: 7 }, (_, i) => f.format(new Date(2026, 10, 1 + ((i + weekStart) % 7))).replace(/\.$/, "").slice(0, lang === "id" ? 3 : 2));
}

export function monthNames(lang: Lang, style: "long" | "short"): string[] {
  const f = new Intl.DateTimeFormat(locale(lang), { month: style });
  return Array.from({ length: 12 }, (_, i) => f.format(new Date(2026, i, 1)).replace(/\.$/, ""));
}

/** "Nov 30, 2026" / "30 Nov 2026"; `withWeekday` adds "Mon, ". */
export function formatLong(iso: Iso, lang: Lang, withWeekday = false): string {
  const d = fromIso(iso);
  if (!d) return iso;
  return d.toLocaleDateString(locale(lang), { ...(withWeekday ? { weekday: "short" } : {}), month: "short", day: "numeric", year: "numeric" });
}

export function formatMonth(iso: Iso, lang: Lang): string {
  return fromIso(iso)!.toLocaleDateString(locale(lang), { month: "long", year: "numeric" });
}

// ── Typed dates ──────────────────────────────────────────────────────────────────────────────
// Both languages are understood whatever the UI language; only the order of a numeric day and
// month follows it (11/30 in English, 30/11 in Indonesian).

const MONTH_WORDS: Record<string, number> = {};
const DAY_WORDS: Record<string, number> = {};
for (const lang of ["en", "id"] as Lang[]) {
  for (const style of ["long", "short"] as const) {
    monthNames(lang, style).forEach((n, i) => (MONTH_WORDS[n.toLowerCase()] = i));
  }
  const long = new Intl.DateTimeFormat(locale(lang), { weekday: "long" });
  const short = new Intl.DateTimeFormat(locale(lang), { weekday: "short" });
  for (let i = 0; i < 7; i++) {
    const d = new Date(2026, 10, 1 + i); // Sunday first
    DAY_WORDS[long.format(d).toLowerCase()] = i;
    DAY_WORDS[short.format(d).toLowerCase().replace(/\.$/, "")] = i;
  }
}
Object.assign(MONTH_WORDS, { sept: 8, agustus: 7, agt: 7, okt: 9, des: 11, mei: 4 });
Object.assign(DAY_WORDS, { tues: 2, thur: 4, thurs: 4, jumat: 5, "jum'at": 5, ahad: 0 });

const RELATIVE: Record<string, number> = {
  today: 0,
  now: 0,
  "hari ini": 0,
  tomorrow: 1,
  tmr: 1,
  besok: 1,
  yesterday: -1,
  kemarin: -1,
  lusa: 2,
  "next week": 7,
  "minggu depan": 7,
};
const RELATIVE_MONTHS: Record<string, number> = { "next month": 1, "bulan depan": 1 };

function valid(y: number, m: number, d: number): Iso | null {
  const iso = `${String(y).padStart(4, "0")}-${pad(m + 1)}-${pad(d)}`;
  return fromIso(iso) ? iso : null;
}

/**
 * Reads what someone typed into a date field. Understands ISO dates (2026-11-30), numeric
 * dates (11/30, 30/11/2026 by language), month names (nov 30, 30 november, 30 nov 2026,
 * "30 mei"), relative words (today, tomorrow, yesterday, hari ini, besok, kemarin, lusa),
 * offsets (+3d, +2w, +1m, -5d; also 3 hari / 2 minggu / 1 bulan), and weekdays (fri,
 * next fri, friday, jumat, senin depan), which mean the next such day after today.
 * Returns null when it cannot tell.
 */
export function parseTyped(text: string, lang: Lang, today: Iso = todayIso()): Iso | null {
  const s = text.trim().toLowerCase().replace(/\s+/g, " ").replace(/[.,]/g, " ").replace(/\s+/g, " ").trim();
  if (!s) return null;
  if (s in RELATIVE) return addDays(today, RELATIVE[s]!);
  if (s in RELATIVE_MONTHS) return addMonths(today, RELATIVE_MONTHS[s]!);

  let m = /^(\d{4})-(\d{1,2})-(\d{1,2})$/.exec(s);
  if (m) return valid(+m[1]!, +m[2]! - 1, +m[3]!);

  m = /^([+-]?)\s?(\d{1,3})\s?(d|day|days|hari|w|wk|week|weeks|minggu|m|mo|month|months|bulan)$/.exec(s);
  if (m && (m[1] || /^[a-z]/.test(m[3]!))) {
    const n = +m[2]! * (m[1] === "-" ? -1 : 1);
    const unit = m[3]!;
    if (/^(d|day|days|hari)$/.test(unit)) return addDays(today, n);
    if (/^(w|wk|week|weeks|minggu)$/.test(unit)) return addDays(today, n * 7);
    return addMonths(today, n);
  }

  m = /^(\d{1,2})[/-](\d{1,2})(?:[/-](\d{2}|\d{4}))?$/.exec(s);
  if (m) {
    const [a, b] = [+m[1]!, +m[2]!];
    const [day, mon] = lang === "id" ? [a, b] : [b, a];
    const y = m[3] ? (m[3].length === 2 ? 2000 + +m[3] : +m[3]) : +today.slice(0, 4);
    return valid(y, mon - 1, day);
  }

  // Weekdays: "fri", "next fri", "friday", "jumat", "senin depan".
  const wd = s.replace(/^(next|this|coming) /, "").replace(/ (depan|ini)$/, "");
  if (wd in DAY_WORDS) {
    const want = DAY_WORDS[wd]!;
    const now = fromIso(today)!.getDay();
    const ahead = ((want - now + 7) % 7) || 7;
    return addDays(today, ahead);
  }

  // Month names: "nov 30", "november 30 2026", "30 nov", "30 november 2026".
  const words = s.split(" ");
  let day: number | null = null;
  let mon: number | null = null;
  let year: number | null = null;
  for (const w of words) {
    if (/^\d{4}$/.test(w)) year = +w;
    else if (/^\d{1,2}(st|nd|rd|th)?$/.test(w)) day = parseInt(w, 10);
    else if (w in MONTH_WORDS) mon = MONTH_WORDS[w]!;
    else return null;
  }
  if (day != null && mon != null) return valid(year ?? +today.slice(0, 4), mon, day);
  return null;
}
