import { createContext, useCallback, useContext, useEffect, useState } from "react";

/** App-frame state shared by the sidebar and page headers. */
export interface Shell {
  collapsed: boolean;
  toggle: () => void;
}

export const ShellContext = createContext<Shell>({ collapsed: false, toggle: () => {} });
export const useShell = () => useContext(ShellContext);

const KEY = "np.sidebar.collapsed";

/** Sidebar open/closed: remembered across visits, closed by default on narrow screens. */
export function useShellState(): Shell {
  const narrow = () => window.matchMedia?.("(max-width: 767px)").matches ?? false;
  const [collapsed, setCollapsed] = useState(() => narrow() || localStorage.getItem(KEY) === "1");
  const toggle = useCallback(() => {
    setCollapsed((c) => {
      if (!narrow()) localStorage.setItem(KEY, c ? "0" : "1");
      return !c;
    });
  }, []);
  useEffect(() => {
    const mq = window.matchMedia?.("(max-width: 767px)");
    if (!mq) return;
    const onChange = () => mq.matches && setCollapsed(true);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return { collapsed, toggle };
}

/** Opens app-level overlays from anywhere (top bar, shortcuts, palette). */
export type ShellEvent = "palette" | "shortcuts" | "create-item";
export const emitShell = (e: ShellEvent, detail?: unknown) => window.dispatchEvent(new CustomEvent(`np:${e}`, { detail }));
export function useShellEvent(e: ShellEvent, fn: (detail: unknown) => void) {
  useEffect(() => {
    const h = (ev: Event) => fn((ev as CustomEvent).detail);
    window.addEventListener(`np:${e}`, h);
    return () => window.removeEventListener(`np:${e}`, h);
  }, [e, fn]);
}
