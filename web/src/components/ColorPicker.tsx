import { Pipette } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useT } from "../i18n";
import { PALETTE } from "../lib/projects";
import { Floating } from "./Floating";

export function ColorPicker({ value, onChange, label }: { value: string; onChange: (c: string) => void; label: string }) {
  return (
    <div className="swatches" role="radiogroup" aria-label={label}>
      {PALETTE.map((c) => (
        <button
          key={c}
          type="button"
          role="radio"
          aria-checked={value === c}
          aria-label={c}
          className={"swatch" + (value === c ? " on" : "")}
          style={{ background: c }}
          onClick={() => onChange(c)}
        />
      ))}
    </div>
  );
}

export function Dot({ color, size = 10 }: { color: string; size?: number }) {
  return <span className="dot" style={{ background: color, width: size, height: size }} aria-hidden />;
}

// ── Free colour: a swatch that opens a picker (state and label colours) ────────────────────────

function hexToHsv(hex: string): [number, number, number] {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  const n = m ? parseInt(m[1]!, 16) : 0x888888;
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((x) => x / 255) as [number, number, number];
  const max = Math.max(r, g, b);
  const d = max - Math.min(r, g, b);
  let h = 0;
  if (d) h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [(h * 60 + 360) % 360, max ? d / max : 0, max];
}

function hsvToHex(h: number, s: number, v: number): string {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return "#" + [r, g, b].map((y) => Math.round((y + m) * 255).toString(16).padStart(2, "0")).join("");
}

const validHex = (s: string) => /^#?[0-9a-f]{6}$/i.test(s.trim());

/** A pointer-and-keyboard 2D or 1D control. `onMove` gets 0..1 positions. */
function dragger(onMove: (x: number, y: number) => void, onEnd: () => void) {
  return (e: React.PointerEvent<HTMLElement>) => {
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const at = (ev: { clientX: number; clientY: number }) => {
      const r = el.getBoundingClientRect();
      onMove(Math.min(1, Math.max(0, (ev.clientX - r.left) / r.width)), Math.min(1, Math.max(0, (ev.clientY - r.top) / r.height)));
    };
    at(e);
    const move = (ev: PointerEvent) => at(ev);
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      onEnd();
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  };
}

export function ColorButton({ value, onChange, label, disabled }: { value: string; onChange: (c: string) => void; label: string; disabled?: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [hsv, setHsv] = useState<[number, number, number]>(() => hexToHsv(value));
  const [hex, setHex] = useState(value.replace("#", "").toUpperCase());
  const btn = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const hsvRef = useRef(hsv);
  hsvRef.current = hsv;
  const current = hsvToHex(...hsv);
  useEffect(() => {
    if (!open) return;
    setHsv(hexToHsv(value));
    setHex(value.replace("#", "").toUpperCase());
    const down = (e: MouseEvent) => {
      const n = e.target as Node;
      if (!btn.current?.contains(n) && !pop.current?.contains(n)) setOpen(false);
    };
    document.addEventListener("mousedown", down);
    return () => document.removeEventListener("mousedown", down);
  }, [open]);
  const commit = (c: string) => {
    const v = c.toLowerCase();
    if (v !== value.toLowerCase()) onChange(v);
  };
  const set = (next: [number, number, number]) => {
    setHsv(next);
    setHex(hsvToHex(...next).slice(1).toUpperCase());
  };
  const sv = dragger((x, y) => set([hsv[0], x, 1 - y]), () => commit(hsvToHex(...hsvRef.current)));
  const hue = dragger((x) => set([x * 360, hsv[1], hsv[2]]), () => commit(hsvToHex(...hsvRef.current)));
  const keys = (e: React.KeyboardEvent, axis: "sv" | "h") => {
    const step = e.shiftKey ? 10 : 2;
    let [h, s, v] = hsv;
    if (axis === "h") {
      if (e.key === "ArrowLeft" || e.key === "ArrowDown") h = (h - step + 360) % 360;
      else if (e.key === "ArrowRight" || e.key === "ArrowUp") h = (h + step) % 360;
      else return;
    } else {
      if (e.key === "ArrowLeft") s = Math.max(0, s - step / 100);
      else if (e.key === "ArrowRight") s = Math.min(1, s + step / 100);
      else if (e.key === "ArrowDown") v = Math.max(0, v - step / 100);
      else if (e.key === "ArrowUp") v = Math.min(1, v + step / 100);
      else return;
    }
    e.preventDefault();
    set([h, s, v]);
    commit(hsvToHex(h, s, v));
  };
  const Eye = (window as unknown as { EyeDropper?: new () => { open: () => Promise<{ sRGBHex: string }> } }).EyeDropper;
  return (
    <>
      <button
        ref={btn}
        type="button"
        className="color-btn"
        style={{ background: value }}
        aria-label={label}
        title={value}
        aria-haspopup="dialog"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
      />
      {open ? (
        <Floating ref={pop} anchor={btn} className="popover cp-pop" onMouseDown={(e) => e.stopPropagation()}>
          <div
            className="cp"
            role="dialog"
            aria-label={label}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                setOpen(false);
                btn.current?.focus();
              }
            }}
          >
            <div
              className="cp-sv"
              role="slider"
              tabIndex={0}
              aria-label={t("cp.sv")}
              aria-valuetext={current}
              aria-valuenow={Math.round(hsv[1] * 100)}
              style={{ background: `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, hsl(${hsv[0]} 100% 50%))` }}
              onPointerDown={sv}
              onKeyDown={(e) => keys(e, "sv")}
            >
              <span className="cp-knob" style={{ left: `${hsv[1] * 100}%`, top: `${(1 - hsv[2]) * 100}%`, background: current }} />
            </div>
            <div className="cp-hue" role="slider" tabIndex={0} aria-label={t("cp.hue")} aria-valuemin={0} aria-valuemax={360} aria-valuenow={Math.round(hsv[0])} onPointerDown={hue} onKeyDown={(e) => keys(e, "h")}>
              <span className="cp-knob" style={{ left: `${(hsv[0] / 360) * 100}%`, background: `hsl(${hsv[0]} 100% 50%)` }} />
            </div>
            <div className="cp-row">
              <span className="cp-preview" style={{ background: current }} />
              <span className="input cp-hex">
                <span className="faint">#</span>
                <input
                  aria-label={t("cp.hex")}
                  value={hex}
                  maxLength={7}
                  onChange={(e) => {
                    const v = e.target.value.replace("#", "").toUpperCase();
                    setHex(v);
                    if (validHex(v)) setHsv(hexToHsv(v));
                  }}
                  onKeyDown={(e) => e.key === "Enter" && validHex(hex) && (e.preventDefault(), commit("#" + hex))}
                  onBlur={() => validHex(hex) && commit("#" + hex)}
                />
              </span>
              {Eye ? (
                <button
                  type="button"
                  className="icon-btn bordered"
                  aria-label={t("cp.eyedropper")}
                  title={t("cp.eyedropper")}
                  onClick={() =>
                    new Eye()
                      .open()
                      .then((r) => {
                        set(hexToHsv(r.sRGBHex));
                        commit(r.sRGBHex);
                      })
                      .catch(() => undefined)
                  }
                >
                  <Pipette />
                </button>
              ) : null}
            </div>
            <div className="cp-swatches" role="radiogroup" aria-label={t("cp.palette")}>
              {PALETTE.map((c) => (
                <button
                  key={c}
                  type="button"
                  role="radio"
                  aria-checked={current === c}
                  aria-label={c}
                  className={"swatch" + (current === c ? " on" : "")}
                  style={{ background: c }}
                  onClick={() => {
                    set(hexToHsv(c));
                    commit(c);
                  }}
                />
              ))}
            </div>
          </div>
        </Floating>
      ) : null}
    </>
  );
}
