import { Check, Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useT } from "../i18n";
import { Floating } from "./Floating";

export interface Option {
  value: string;
  label: string;
  icon?: ReactNode;
  hint?: string;
}

/** A button that opens a searchable menu. Single choice closes on pick; `multi` toggles.
 * Keyboard: ↑ ↓ to move, Enter to pick, Esc to close. */
export function Picker({
  label,
  options,
  value,
  onChange,
  multi,
  trigger,
  className = "prop-btn",
  disabled,
  allowClear,
  searchable = true,
  align = "left",
  autoOpen,
  onClosed,
  title,
}: {
  label: string;
  options: Option[];
  value: string[];
  onChange: (v: string[]) => void;
  multi?: boolean;
  trigger: ReactNode;
  className?: string;
  disabled?: boolean;
  allowClear?: string;
  searchable?: boolean;
  align?: "left" | "right";
  /** Open as soon as it mounts (e.g. a filter that was just added). */
  autoOpen?: boolean;
  onClosed?: () => void;
  title?: string;
}) {
  const t = useT();
  const [open, setOpenRaw] = useState(!!autoOpen);
  const setOpen = (o: boolean | ((x: boolean) => boolean)) =>
    setOpenRaw((cur) => {
      const next = typeof o === "function" ? o(cur) : o;
      if (cur && !next) onClosed?.();
      return next;
    });
  const [q, setQ] = useState("");
  const [focus, setFocus] = useState(0);
  const wrap = useRef<HTMLDivElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const shown = useMemo(() => {
    const ql = q.trim().toLowerCase();
    const list = ql ? options.filter((o) => o.label.toLowerCase().includes(ql) || o.hint?.toLowerCase().includes(ql)) : options;
    return allowClear && !ql ? [{ value: "", label: allowClear }, ...list] : list;
  }, [q, options, allowClear]);

  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => {
      const t = e.target as Node;
      if (!wrap.current?.contains(t) && !pop.current?.contains(t)) setOpen(false);
    };
    document.addEventListener("mousedown", down);
    return () => document.removeEventListener("mousedown", down);
  }, [open]);
  useEffect(() => setFocus(0), [q, open]);

  const pick = (v: string) => {
    if (multi) {
      onChange(value.includes(v) ? value.filter((x) => x !== v) : [...value, v]);
    } else {
      onChange(v ? [v] : []);
      setOpen(false);
    }
  };
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setFocus((f) => Math.min(f + 1, shown.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setFocus((f) => Math.max(f - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const o = shown[focus];
      if (o) pick(o.value);
    } else if (e.key === "Escape") {
      e.stopPropagation();
      setOpen(false);
    }
  };

  return (
    <div className="popover-wrap" ref={wrap} onKeyDown={open ? onKey : undefined}>
      <button
        type="button"
        className={className}
        disabled={disabled}
        aria-label={label}
        title={title}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={(e) => {
          e.stopPropagation();
          setQ("");
          setOpen((o) => !o);
        }}
      >
        {trigger}
      </button>
      {open ? (
        <Floating ref={pop} anchor={wrap} align={align === "right" ? "end" : "start"} className="popover" onClick={(e) => e.stopPropagation()}>
          <div className="menu picker-menu">
            {searchable ? (
              <div className="menu-search-row">
                <Search aria-hidden />
                <input className="menu-search" autoFocus placeholder={t("common.search")} value={q} onChange={(e) => setQ(e.target.value)} aria-label={t("common.search")} />
              </div>
            ) : null}
            <div role="listbox" aria-label={label} aria-multiselectable={multi} tabIndex={searchable ? -1 : 0} autoFocus={!searchable} style={{ outline: "none" }}>
              {shown.length === 0 ? <div className="menu-label">{t("common.noResults")}</div> : null}
              {shown.map((o, i) => {
                const on = o.value ? value.includes(o.value) : value.length === 0;
                return (
                  <button
                    key={o.value || "__clear"}
                    type="button"
                    role="option"
                    aria-selected={on}
                    className={"menu-item" + (i === focus ? " focused" : "")}
                    onMouseEnter={() => setFocus(i)}
                    onClick={() => pick(o.value)}
                  >
                    {multi ? <span className={"menu-checkbox" + (on ? " on" : "")} aria-hidden>{on ? <Check /> : null}</span> : null}
                    {o.icon ?? null}
                    <span className="grow ellipsis">{o.label}</span>
                    {o.hint ? <span className="faint small">{o.hint}</span> : null}
                    {multi ? null : on ? <Check className="menu-check" aria-hidden /> : null}
                  </button>
                );
              })}
            </div>
          </div>
        </Floating>
      ) : null}
    </div>
  );
}
