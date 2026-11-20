// Analytics charts (dataviz skill rules): one hue for single-series bars, categorical
// slots in fixed order for segments ("Other" in gray), bars at most 18px thick with a 4px
// rounded end and a square baseline, 2px gaps between stacked segments, 2px lines, hairline
// grid, legends for two or more series, a tooltip on hover and keyboard focus, and every
// chart has a table view with the same numbers.
import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { useI18n } from "../../i18n";

export interface Bar {
  key: string;
  label: string;
  value: number;
  icon?: ReactNode;
}

export interface Segment {
  key: string;
  label: string;
  /** Color slot (0-7). Ordered dimensions pass a fixed slot per value so a value keeps its
   * color when others come and go; otherwise the position in the legend. */
  slot?: number;
}

export const fmtNum = (v: number, lang: string) =>
  new Intl.NumberFormat(lang === "id" ? "id-ID" : "en-US", { maximumFractionDigits: 1, notation: Math.abs(v) >= 10000 ? "compact" : "standard" }).format(v);

/** Series color of a segment: slots in order, "Other" gray. */
export function seriesColor(index: number, key: string): string {
  if (key === "__other") return "var(--series-other)";
  return `var(--series-${(index % 8) + 1})`;
}

/** A card holding one chart and its table view. */
export function ChartCard({ title, subtitle, table, children }: { title: string; subtitle?: string; table: ReactNode; children: ReactNode }) {
  const { t } = useI18n();
  const [asTable, setAsTable] = useState(false);
  const id = useId();
  return (
    <figure className="chart-card card" aria-labelledby={id}>
      <div className="chart-head">
        <div>
          <h3 id={id} className="chart-title">{title}</h3>
          {subtitle ? <div className="faint small">{subtitle}</div> : null}
        </div>
        <button className="btn btn-sm btn-ghost" aria-pressed={asTable} onClick={() => setAsTable((v) => !v)}>
          {asTable ? t("an.chart") : t("an.table")}
        </button>
      </div>
      {asTable ? <div className="chart-table-wrap">{table}</div> : children}
    </figure>
  );
}

export function StatTile({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="stat-tile card">
      <div className="stat-label">{label}</div>
      <div className="stat-value">{value}</div>
      {hint ? <div className="faint small">{hint}</div> : null}
    </div>
  );
}

interface Tip {
  x: number;
  y: number;
  lines: { key?: string; color?: string; value: string; label: string }[];
  title?: string;
}

function Tooltip({ tip }: { tip: Tip | null }) {
  if (!tip) return null;
  return (
    <div className="chart-tip" role="status" style={{ left: tip.x, top: tip.y }}>
      {tip.title ? <div className="chart-tip-title">{tip.title}</div> : null}
      {tip.lines.map((l, i) => (
        <div key={i} className="chart-tip-row">
          {l.color ? <span className="chart-key-line" style={{ background: l.color }} /> : null}
          <strong>{l.value}</strong>
          <span className="muted">{l.label}</span>
        </div>
      ))}
    </div>
  );
}

/** Where to put a tooltip for an element inside `box`. */
function tipAt(box: HTMLElement | null, el: Element, dx = 0): { x: number; y: number } {
  const b = box?.getBoundingClientRect();
  const r = el.getBoundingClientRect();
  return { x: (b ? r.left - b.left : 0) + dx, y: b ? r.bottom - b.top + 6 : 0 };
}

/** Horizontal bars of one series: one hue, value at the tip. */
export function BarList({ rows, total, empty }: { rows: Bar[]; total: number; empty: string }) {
  const { lang } = useI18n();
  const [tip, setTip] = useState<Tip | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const max = Math.max(0, ...rows.map((r) => r.value));
  if (!rows.length || max === 0) return <p className="faint small chart-empty">{empty}</p>;
  const show = (el: Element, r: Bar) => {
    const share = total > 0 ? ` · ${Math.round((r.value / total) * 100)}%` : "";
    setTip({ ...tipAt(box.current, el), lines: [{ value: fmtNum(r.value, lang) + share, label: r.label }] });
  };
  return (
    <div className="bar-list" ref={box} onPointerLeave={() => setTip(null)}>
      {rows.map((r) => (
        <div
          key={r.key}
          className="bar-row"
          tabIndex={0}
          aria-label={`${r.label}: ${fmtNum(r.value, lang)}`}
          onPointerEnter={(e) => show(e.currentTarget.querySelector(".bar-fill") ?? e.currentTarget, r)}
          onFocus={(e) => show(e.currentTarget.querySelector(".bar-fill") ?? e.currentTarget, r)}
          onBlur={() => setTip(null)}
        >
          <span className="bar-label">
            {r.icon}
            <span className="ellipsis">{r.label}</span>
          </span>
          <span className="bar-track">
            {r.value > 0 ? <span className="bar-fill" style={{ width: `${(r.value / max) * 100}%`, background: "var(--series-1)" }} /> : null}
            <span className="bar-value">{fmtNum(r.value, lang)}</span>
          </span>
        </div>
      ))}
      <Tooltip tip={tip} />
    </div>
  );
}

/** Horizontal stacked bars: rows split into segments, with a legend in segment order. */
export function StackedBars({ rows, segments, cells, empty }: { rows: Bar[]; segments: Segment[]; cells: Map<string, number>; empty: string }) {
  const { t, lang } = useI18n();
  const [tip, setTip] = useState<Tip | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const max = Math.max(0, ...rows.map((r) => r.value));
  if (!rows.length || max === 0) return <p className="faint small chart-empty">{empty}</p>;
  return (
    <div ref={box} className="stacked" onPointerLeave={() => setTip(null)}>
      <ul className="chart-legend" aria-label={t("an.legend")}>
        {segments.map((s, i) => (
          <li key={s.key}>
            <span className="chart-swatch" style={{ background: seriesColor(s.slot ?? i, s.key) }} />
            {s.label}
          </li>
        ))}
      </ul>
      <div className="bar-list">
        {rows.map((r) => (
          <div key={r.key} className="bar-row">
            <span className="bar-label">
              {r.icon}
              <span className="ellipsis">{r.label}</span>
            </span>
            <span className="bar-track">
              <span className="stack" style={{ width: `${(r.value / max) * 100}%` }}>
                {segments.map((s, i) => {
                  const v = cells.get(`${r.key}\u0000${s.key}`) ?? 0;
                  if (v <= 0) return null;
                  const show = (el: Element) =>
                    setTip({ ...tipAt(box.current, el), title: r.label, lines: [{ color: seriesColor(s.slot ?? i, s.key), value: fmtNum(v, lang), label: s.label }, { value: fmtNum(r.value, lang), label: t("an.total") }] });
                  return (
                    <span
                      key={s.key}
                      className="stack-seg"
                      tabIndex={0}
                      aria-label={`${r.label}, ${s.label}: ${fmtNum(v, lang)}`}
                      style={{ flexGrow: v, background: seriesColor(s.slot ?? i, s.key) }}
                      onPointerEnter={(e) => show(e.currentTarget)}
                      onFocus={(e) => show(e.currentTarget)}
                      onBlur={() => setTip(null)}
                    />
                  );
                })}
              </span>
              <span className="bar-value">{fmtNum(r.value, lang)}</span>
            </span>
          </div>
        ))}
      </div>
      <Tooltip tip={tip} />
    </div>
  );
}

export interface TrendPoint {
  label: string;
  values: number[];
}

const TH = 220;
const TP = { top: 14, right: 92, bottom: 28, left: 36 };

/** A rounded top whose half is a whole number too (counts have no 2.5). */
function niceMax(v: number): number {
  if (v <= 2) return 2;
  const step = Math.pow(10, Math.floor(Math.log10(v)));
  for (const m of [1, 2, 4, 6, 8, 10]) if (m * step >= v && Number.isInteger((m * step) / 2)) return m * step;
  return 10 * step;
}

/** The rendered width of an element, so SVG text stays at its real pixel size. */
function useWidth<T extends Element>(fallback: number) {
  const ref = useRef<T>(null);
  const [w, setW] = useState(fallback);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(([e]) => e && setW(Math.max(240, Math.round(e.contentRect.width))));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, w] as const;
}

/** A few series over time on one axis: 2px lines, end dots, crosshair tooltip. */
export function TrendLines({ points, series, xLabel }: { points: TrendPoint[]; series: string[]; xLabel: (i: number) => string }) {
  const { lang } = useI18n();
  const [hover, setHover] = useState<number | null>(null);
  const svg = useRef<SVGSVGElement>(null);
  const [plot, TW] = useWidth<HTMLDivElement>(640);
  const n = points.length;
  const max = useMemo(() => niceMax(Math.max(1, ...points.flatMap((p) => p.values))), [points]);
  const x = (i: number) => TP.left + (n <= 1 ? 0 : (i / (n - 1)) * (TW - TP.left - TP.right));
  const y = (v: number) => TP.top + (1 - v / max) * (TH - TP.top - TP.bottom);
  const ticks = [0, max / 2, max];
  const every = Math.max(1, Math.ceil(n / Math.max(2, Math.floor(TW / 80))));
  const ends = series.map((_, s) => y(points[n - 1]?.values[s] ?? 0));
  // Direct end labels only where they do not collide; the legend always names the lines.
  const labelOk = ends.map((e, s) => ends.every((o, j) => j === s || Math.abs(o - e) >= 14));
  const pick = (clientX: number) => {
    const r = svg.current?.getBoundingClientRect();
    if (!r || n === 0) return;
    const sx = ((clientX - r.left) / r.width) * TW;
    const i = Math.round(((sx - TP.left) / (TW - TP.left - TP.right)) * (n - 1));
    setHover(Math.max(0, Math.min(n - 1, i)));
  };
  return (
    <div className="trend">
      <ul className="chart-legend">
        {series.map((s, i) => (
          <li key={s}>
            <span className="chart-key-line" style={{ background: seriesColor(i, s) }} />
            {s}
          </li>
        ))}
      </ul>
      <div className="trend-plot" ref={plot}>
        <svg
          ref={svg}
          viewBox={`0 0 ${TW} ${TH}`}
          className="trend-svg"
          role="img"
          aria-label={series.join(", ")}
          tabIndex={0}
          onPointerMove={(e) => pick(e.clientX)}
          onPointerLeave={() => setHover(null)}
          onFocus={() => setHover(n - 1)}
          onBlur={() => setHover(null)}
          onKeyDown={(e) => {
            if (e.key === "ArrowLeft") setHover((h) => Math.max(0, (h ?? n - 1) - 1));
            if (e.key === "ArrowRight") setHover((h) => Math.min(n - 1, (h ?? 0) + 1));
          }}
        >
          {ticks.map((v) => (
            <g key={v}>
              <line x1={TP.left} x2={TW - TP.right} y1={y(v)} y2={y(v)} stroke={v === 0 ? "var(--chart-axis)" : "var(--chart-grid)"} strokeWidth={1} />
              <text x={TP.left - 6} y={y(v) + 4} textAnchor="end" className="chart-tick">{fmtNum(v, lang)}</text>
            </g>
          ))}
          {points.map((_, i) =>
            (n - 1 - i) % every === 0 ? (
              <text key={i} x={x(i)} y={TH - 8} textAnchor="middle" className="chart-tick">{xLabel(i)}</text>
            ) : null,
          )}
          {hover !== null ? <line x1={x(hover)} x2={x(hover)} y1={TP.top} y2={TH - TP.bottom} stroke="var(--text-faint)" strokeWidth={1} /> : null}
          {series.map((s, si) => (
            <g key={s}>
              <path
                d={points.map((p, i) => `${i ? "L" : "M"}${x(i)},${y(p.values[si] ?? 0)}`).join("")}
                fill="none"
                stroke={seriesColor(si, s)}
                strokeWidth={2}
                strokeLinejoin="round"
                strokeLinecap="round"
              />
              {n ? <circle cx={x(n - 1)} cy={ends[si]} r={4} fill={seriesColor(si, s)} stroke="var(--bg)" strokeWidth={2} /> : null}
              {n && labelOk[si] ? (
                <text x={x(n - 1) + 9} y={(ends[si] ?? 0) + 4} className="chart-end-label">{s}</text>
              ) : null}
              {hover !== null ? <circle cx={x(hover)} cy={y(points[hover]?.values[si] ?? 0)} r={4} fill={seriesColor(si, s)} stroke="var(--bg)" strokeWidth={2} /> : null}
            </g>
          ))}
        </svg>
        {hover !== null && points[hover] ? (
          <div className="chart-tip" role="status" style={{ left: `${(x(hover) / TW) * 100}%`, top: 8 }}>
            <div className="chart-tip-title">{xLabel(hover)}</div>
            {series.map((s, si) => (
              <div key={s} className="chart-tip-row">
                <span className="chart-key-line" style={{ background: seriesColor(si, s) }} />
                <strong>{fmtNum(points[hover]!.values[si] ?? 0, lang)}</strong>
                <span className="muted">{s}</span>
              </div>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

/** The table view of any chart. */
export function DataTable({ head, rows }: { head: string[]; rows: (string | number)[][] }) {
  return (
    <table className="data-table">
      <thead>
        <tr>
          {head.map((h, i) => (
            <th key={i} scope="col" className={i ? "num" : ""}>{h}</th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr key={i}>
            {r.map((c, j) => (j === 0 ? <th key={j} scope="row">{c}</th> : <td key={j} className="num">{c}</td>))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
