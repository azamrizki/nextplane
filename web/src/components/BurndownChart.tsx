// Burn-down: remaining open items per day against the ideal straight line. Two series,
// one y-axis, 2px lines, hairline grid, legend + direct end label, crosshair tooltip on
// hover and keyboard, and the same numbers as a table (dataviz skill rules).
import { useMemo, useRef, useState } from "react";
import { useI18n } from "../i18n";
import { fmtDate } from "./Props";

export interface BurnPoint {
  date: string;
  ideal: number;
  remaining: number | null;
}

const W = 640;
const H = 220;
const PAD = { top: 12, right: 72, bottom: 28, left: 36 };

const fmt = (v: number) => (Number.isInteger(v) ? String(v) : v.toFixed(1));

function niceMax(v: number): number {
  if (v <= 5) return 5;
  const step = Math.pow(10, Math.floor(Math.log10(v)));
  for (const m of [1, 2, 2.5, 5, 10]) if (m * step >= v) return m * step;
  return 10 * step;
}

export function BurndownChart({ points }: { points: BurnPoint[] }) {
  const { t, lang } = useI18n();
  const [hover, setHover] = useState<number | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const n = points.length;
  const max = useMemo(() => niceMax(Math.max(1, ...points.map((p) => Math.max(p.ideal, p.remaining ?? 0)))), [points]);
  const x = (i: number) => PAD.left + (n <= 1 ? 0 : (i / (n - 1)) * (W - PAD.left - PAD.right));
  const y = (v: number) => PAD.top + (1 - v / max) * (H - PAD.top - PAD.bottom);
  // Only whole-number ticks: a middle tick only when max/2 is a whole number.
  const ticks = Number.isInteger(max / 2) ? [0, max / 2, max] : [0, max];
  const ideal = points.map((p, i) => `${i ? "L" : "M"}${x(i)},${y(p.ideal)}`).join("");
  const actual = points.filter((p) => p.remaining !== null);
  const remaining = actual.map((p, i) => `${i ? "L" : "M"}${x(i)},${y(p.remaining!)}`).join("");
  const last = actual.length - 1;
  const xLabels = n <= 1 ? [0] : [0, Math.floor((n - 1) / 2), n - 1];

  const onMove = (e: React.PointerEvent<SVGSVGElement>) => {
    const r = svgRef.current!.getBoundingClientRect();
    const px = ((e.clientX - r.left) / r.width) * W;
    const i = Math.round(((px - PAD.left) / (W - PAD.left - PAD.right)) * (n - 1));
    setHover(Math.max(0, Math.min(n - 1, i)));
  };
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowRight") setHover((h) => Math.min(n - 1, (h ?? -1) + 1));
    if (e.key === "ArrowLeft") setHover((h) => Math.max(0, (h ?? n) - 1));
  };
  const hp = hover !== null ? points[hover] : null;

  return (
    <figure className="chart">
      <div className="chart-legend" aria-hidden>
        <span><i className="key" style={{ background: "var(--chart-remaining)" }} />{t("cyc.remaining")}</span>
        <span><i className="key" style={{ background: "var(--chart-ideal)" }} />{t("cyc.ideal")}</span>
      </div>
      <div className="chart-plot">
        <svg
          ref={svgRef}
          viewBox={`0 0 ${W} ${H}`}
          role="img"
          aria-label={t("cyc.chartLabel")}
          tabIndex={0}
          onPointerMove={onMove}
          onPointerLeave={() => setHover(null)}
          onFocus={() => setHover((h) => h ?? Math.max(0, last))}
          onBlur={() => setHover(null)}
          onKeyDown={onKey}
        >
          {ticks.map((v) => (
            <g key={v}>
              <line x1={PAD.left} x2={W - PAD.right} y1={y(v)} y2={y(v)} stroke="var(--chart-grid)" strokeWidth={1} />
              <text x={PAD.left - 8} y={y(v)} dy="0.32em" textAnchor="end" className="chart-tick">{v}</text>
            </g>
          ))}
          {xLabels.map((i) => (
            <text key={i} x={x(i)} y={H - 8} textAnchor={i === 0 ? "start" : i === n - 1 ? "end" : "middle"} className="chart-tick">
              {fmtDate(points[i]!.date, lang)}
            </text>
          ))}
          <path d={ideal} fill="none" stroke="var(--chart-ideal)" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" />
          {remaining ? <path d={remaining} fill="none" stroke="var(--chart-remaining)" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" /> : null}
          {last >= 0 ? (
            <>
              <circle cx={x(last)} cy={y(actual[last]!.remaining!)} r={4} fill="var(--chart-remaining)" stroke="var(--bg)" strokeWidth={2} />
              <text x={x(last) + 10} y={y(actual[last]!.remaining!)} dy="0.32em" className="chart-label">
                {actual[last]!.remaining} {t("cyc.remaining").toLowerCase()}
              </text>
            </>
          ) : null}
          {hover !== null ? <line x1={x(hover)} x2={x(hover)} y1={PAD.top} y2={H - PAD.bottom} stroke="var(--border-strong)" strokeWidth={1} /> : null}
        </svg>
        {hp ? (
          <div className="chart-tip" style={{ left: `${(x(hover!) / W) * 100}%` }} role="status">
            <div className="faint small">{fmtDate(hp.date, lang)}</div>
            <div><i className="key line" style={{ background: "var(--chart-remaining)" }} /><b>{hp.remaining ?? "—"}</b> <span className="muted">{t("cyc.remaining")}</span></div>
            <div><i className="key line" style={{ background: "var(--chart-ideal)" }} /><b>{fmt(hp.ideal)}</b> <span className="muted">{t("cyc.ideal")}</span></div>
          </div>
        ) : null}
      </div>
      <details className="chart-table">
        <summary className="small muted">{t("cyc.table")}</summary>
        <table>
          <thead>
            <tr><th>{t("cyc.date")}</th><th>{t("cyc.remaining")}</th><th>{t("cyc.ideal")}</th></tr>
          </thead>
          <tbody>
            {points.map((p) => (
              <tr key={p.date}><td>{fmtDate(p.date, lang)}</td><td>{p.remaining ?? "—"}</td><td>{fmt(p.ideal)}</td></tr>
            ))}
          </tbody>
        </table>
      </details>
    </figure>
  );
}
