import { Minus, Plus } from "lucide-react";
import { useT } from "../i18n";

/** A whole-number field with − and + buttons (the browser's spinner is hidden). The input comes
 * first in the DOM, so a wrapping <label> labels the input, not a button. `value` is the text,
 * as typed. */
export function NumberField({
  value,
  onChange,
  min,
  max,
  id,
  unit,
  autoFocus,
  disabled,
  "aria-label": ariaLabel,
}: {
  value: string;
  onChange: (v: string) => void;
  min?: number;
  max?: number;
  id?: string;
  unit?: string;
  autoFocus?: boolean;
  disabled?: boolean;
  "aria-label"?: string;
}) {
  const t = useT();
  const n = Number(value);
  const clamp = (x: number) => Math.min(max ?? Infinity, Math.max(min ?? -Infinity, x));
  const bump = (d: number) => onChange(String(clamp((Number.isFinite(n) && value !== "" ? n : (min ?? 0) - d) + d)));
  return (
    <span className={"stepper" + (disabled ? " disabled" : "")}>
      <input
        id={id}
        className="stepper-input"
        type="number"
        inputMode="numeric"
        min={min}
        max={max}
        step={1}
        value={value}
        autoFocus={autoFocus}
        disabled={disabled}
        aria-label={ariaLabel}
        onChange={(e) => onChange(e.target.value)}
      />
      {unit ? <span className="stepper-unit">{unit}</span> : null}
      <button type="button" className="stepper-btn less" tabIndex={-1} aria-label={t("num.less")} disabled={disabled || (min != null && n <= min)} onClick={() => bump(-1)}>
        <Minus aria-hidden />
      </button>
      <button type="button" className="stepper-btn more" tabIndex={-1} aria-label={t("num.more")} disabled={disabled || (max != null && n >= max)} onClick={() => bump(1)}>
        <Plus aria-hidden />
      </button>
    </span>
  );
}
