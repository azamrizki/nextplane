import { Check, ChevronsUpDown, Search } from "lucide-react";
import { Children, Fragment, isValidElement, useEffect, useId, useMemo, useRef, useState, type ReactElement, type ReactNode } from "react";
import { useT } from "../i18n";
import { Floating } from "./Floating";

export interface SelectOption {
  value: string;
  label: string;
  description?: string;
  icon?: ReactNode;
  disabled?: boolean;
  /** Options with the same group are listed under its name, like an <optgroup>. */
  group?: string;
}

/** The text inside an <option> (strings, numbers and nested elements). */
function textOf(node: ReactNode): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (isValidElement(node)) return textOf((node.props as { children?: ReactNode }).children);
  return "";
}

/** <option> and <optgroup> children, read like a native select reads them. */
function optionsFrom(children: ReactNode, group?: string): SelectOption[] {
  const out: SelectOption[] = [];
  Children.forEach(children, (c) => {
    if (!isValidElement(c)) return;
    const el = c as ReactElement<{ value?: string | number; children?: ReactNode; disabled?: boolean; label?: string }>;
    if (el.type === Fragment) out.push(...optionsFrom(el.props.children, group));
    else if (el.type === "optgroup") out.push(...optionsFrom(el.props.children, el.props.label));
    else if (el.type === "option") {
      const label = textOf(el.props.children);
      out.push({ value: String(el.props.value ?? label), label, disabled: el.props.disabled, group });
    }
  });
  return out;
}

const PAGE = 8;

/**
 * Our own select: a button that opens a frosted listbox in the floating layer (never cut off).
 * Takes `options`, or <option>/<optgroup> children like a native select; `onChange` gets the
 * value. Keyboard as a native select: ↑ ↓ Home End PgUp PgDn, Enter or Space to pick, Esc,
 * and typing jumps to the next option starting with those letters (also while closed). A
 * search field appears when there are more than eight options.
 */
export function Select({
  value,
  onChange,
  options,
  children,
  id,
  className = "",
  disabled,
  required,
  placeholder,
  searchable,
  title,
  "aria-label": ariaLabel,
  "aria-labelledby": ariaLabelledBy,
}: {
  value: string;
  onChange: (value: string) => void;
  options?: SelectOption[];
  children?: ReactNode;
  id?: string;
  className?: string;
  disabled?: boolean;
  required?: boolean;
  placeholder?: string;
  searchable?: boolean;
  title?: string;
  "aria-label"?: string;
  "aria-labelledby"?: string;
}) {
  const t = useT();
  const all = useMemo(() => options ?? optionsFrom(children), [options, children]);
  const current = all.find((o) => o.value === value);
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [active, setActive] = useState(0);
  const btn = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const typed = useRef({ text: "", at: 0 });
  const listId = useId();
  const withSearch = searchable ?? all.length > 8;
  const shown = useMemo(() => {
    const ql = q.trim().toLowerCase();
    return ql ? all.filter((o) => o.label.toLowerCase().includes(ql) || o.description?.toLowerCase().includes(ql)) : all;
  }, [all, q]);

  const enabled = (i: number) => i >= 0 && i < shown.length && !shown[i]!.disabled;
  const step = (from: number, dir: 1 | -1, list = shown) => {
    for (let i = from + dir; i >= 0 && i < list.length; i += dir) if (!list[i]!.disabled) return i;
    return from;
  };
  const show = () => {
    if (disabled) return;
    setQ("");
    const i = all.findIndex((o) => o.value === value);
    setActive(i >= 0 ? i : step(-1, 1, all));
    setOpen(true);
  };
  const close = (refocus = true) => {
    setOpen(false);
    if (refocus) btn.current?.focus();
  };
  const choose = (o: SelectOption | undefined) => {
    if (!o || o.disabled) return;
    if (o.value !== value) onChange(o.value);
    close();
  };

  // Click outside closes; the listbox lives in a portal, so check both.
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => {
      const n = e.target as Node;
      if (!btn.current?.contains(n) && !pop.current?.contains(n)) close(false);
    };
    document.addEventListener("mousedown", down);
    return () => document.removeEventListener("mousedown", down);
  }, [open]);
  useEffect(() => {
    if (open) setActive((a) => (enabled(a) ? a : step(-1, 1)));
  }, [q]);
  useEffect(() => {
    if (!open) return;
    list.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [open, active]);

  /** Type-ahead: the next option whose label starts with what was typed in the last 600 ms. */
  const typeAhead = (key: string) => {
    const now = Date.now();
    const tt = typed.current;
    tt.text = now - tt.at > 600 ? key : tt.text + key;
    tt.at = now;
    const needle = tt.text.toLowerCase();
    const pool = open ? shown : all;
    const from = open ? active : pool.findIndex((o) => o.value === value);
    const order = [...pool.keys()].map((k) => (from + (tt.text.length === 1 ? 1 : 0) + k) % pool.length);
    const hit = order.find((i) => !pool[i]!.disabled && pool[i]!.label.toLowerCase().startsWith(needle));
    if (hit == null) return;
    if (open) setActive(hit);
    else if (pool[hit]!.value !== value) onChange(pool[hit]!.value);
  };

  const onKey = (e: React.KeyboardEvent) => {
    const k = e.key;
    if (!open) {
      if (k === "ArrowDown" || k === "ArrowUp" || k === "Enter" || k === " ") {
        e.preventDefault();
        show();
      } else if (k.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        typeAhead(k);
      }
      return;
    }
    const inSearch = (e.target as HTMLElement).tagName === "INPUT";
    if (k === "ArrowDown") (e.preventDefault(), setActive((a) => step(a, 1)));
    else if (k === "ArrowUp") (e.preventDefault(), setActive((a) => step(a, -1)));
    else if (k === "Home" && !inSearch) (e.preventDefault(), setActive(step(-1, 1)));
    else if (k === "End" && !inSearch) (e.preventDefault(), setActive(step(shown.length, -1)));
    else if (k === "PageDown") (e.preventDefault(), setActive((a) => { let i = a; for (let n = 0; n < PAGE; n++) i = step(i, 1); return i; }));
    else if (k === "PageUp") (e.preventDefault(), setActive((a) => { let i = a; for (let n = 0; n < PAGE; n++) i = step(i, -1); return i; }));
    else if (k === "Enter" || (k === " " && !inSearch)) (e.preventDefault(), choose(shown[active]));
    else if (k === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (k === "Tab") close(false);
    else if (!inSearch && k.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) typeAhead(k);
  };

  let lastGroup: string | undefined;
  return (
    <>
      <button
        ref={btn}
        type="button"
        id={id}
        className={"select select-trigger" + (className ? " " + className : "")}
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-activedescendant={open && !withSearch && shown[active] ? `${listId}-${active}` : undefined}
        aria-label={ariaLabel}
        aria-labelledby={ariaLabelledBy}
        aria-required={required || undefined}
        data-value={value}
        disabled={disabled}
        title={title}
        onClick={() => (open ? close() : show())}
        onKeyDown={onKey}
      >
        {current?.icon ?? null}
        <span className={"select-value" + (current ? "" : " placeholder")}>{current?.label ?? placeholder ?? ""}</span>
        <ChevronsUpDown className="select-chev" aria-hidden />
      </button>
      {required ? (
        // Keeps the browser's "please fill in" check for required choices.
        <input className="select-native" tabIndex={-1} aria-hidden required value={value} onChange={() => undefined} />
      ) : null}
      {open ? (
        <Floating
          ref={pop}
          anchor={btn}
          className="popover select-pop"
          // Inside another popover (the display menu), a pick must not count as a click outside it.
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => e.stopPropagation()}
        >
          <div className="menu select-menu" style={{ minWidth: btn.current?.offsetWidth }} onKeyDown={onKey}>
            {withSearch ? (
              <div className="menu-search-row">
                <Search aria-hidden />
                <input
                  className="menu-search"
                  autoFocus
                  placeholder={t("common.search")}
                  aria-label={t("common.search")}
                  aria-controls={listId}
                  aria-activedescendant={shown[active] ? `${listId}-${active}` : undefined}
                  value={q}
                  onChange={(e) => setQ(e.target.value)}
                />
              </div>
            ) : null}
            <div ref={list} id={listId} role="listbox" aria-label={ariaLabel ?? current?.label}>
              {shown.length === 0 ? <div className="menu-label">{t("common.noResults")}</div> : null}
              {shown.map((o, i) => {
                const head = o.group && o.group !== lastGroup ? o.group : null;
                lastGroup = o.group;
                const on = o.value === value;
                return (
                  <Fragment key={o.value + "\u0000" + i}>
                    {head ? (
                      <div className="menu-label" role="presentation">
                        {head}
                      </div>
                    ) : null}
                    <div
                      id={`${listId}-${i}`}
                      role="option"
                      aria-selected={on}
                      aria-disabled={o.disabled || undefined}
                      data-index={i}
                      data-value={o.value}
                      className={"menu-item select-option" + (i === active ? " focused" : "") + (on ? " active" : "") + (o.description ? " menu-item-2line" : "")}
                      onMouseEnter={() => !o.disabled && setActive(i)}
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => choose(o)}
                    >
                      {o.icon ?? null}
                      {o.description ? (
                        <span className="menu-item-text grow">
                          <span className="ellipsis">{o.label}</span>
                          <span className="menu-item-sub">{o.description}</span>
                        </span>
                      ) : (
                        <span className="grow ellipsis">{o.label}</span>
                      )}
                      {on ? <Check className="menu-check" aria-hidden /> : null}
                    </div>
                  </Fragment>
                );
              })}
            </div>
          </div>
        </Floating>
      ) : null}
    </>
  );
}
