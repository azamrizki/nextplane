import { forwardRef, useCallback, useImperativeHandle, useLayoutEffect, useRef, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";

const GAP = 4;
const MARGIN = 8;

/** Where a floating layer goes: inside the open modal dialog its anchor is in (a modal dialog
 * sits in the browser's top layer, above anything outside it), else the document body. */
export function layerHost(anchor: Element | null): HTMLElement {
  return (anchor?.closest("dialog[open]") as HTMLElement | null) ?? document.body;
}

/**
 * Places `f` next to `anchor` in viewport coordinates: below (or above when there is more
 * room there, or when `up` asks for it), lined up with the anchor's start or end, kept inside
 * the window. When neither side has room it slides over its trigger to stay whole, and only a
 * layer taller than the window gets a height limit (it scrolls inside). Used for menus and
 * pickers so no scrolling or clipping container (board columns, list rows, panels, dialogs)
 * can cut them off.
 */
export function placeNear(anchor: DOMRect, f: HTMLElement, align: "start" | "end", up: boolean) {
  f.style.removeProperty("--float-max");
  const w = f.offsetWidth;
  const h = f.offsetHeight;
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const below = vh - anchor.bottom - GAP - MARGIN;
  const above = anchor.top - GAP - MARGIN;
  const goUp = up ? above >= Math.min(h, 160) || above > below : h > below && above > below;
  let room = Math.max(96, goUp ? above : below);
  const height = Math.min(h, room);
  let top = goUp ? anchor.top - GAP - height : anchor.bottom + GAP;
  // Neither side has room but the window does (a big panel such as the range picker on a short
  // screen): keep it whole and slide it over its trigger rather than make it scroll.
  if (h > room && h <= vh - 2 * MARGIN) {
    top = Math.max(MARGIN, Math.min(goUp ? anchor.top - GAP - h : anchor.bottom + GAP, vh - MARGIN - h));
    room = h;
  }
  let left = align === "end" ? anchor.right - w : anchor.left;
  left = Math.max(MARGIN, Math.min(left, vw - w - MARGIN));
  // The menu inside scrolls (its border stays whole); see `.floating` in base.css.
  if (h > room) f.style.setProperty("--float-max", `${room}px`);
  f.style.left = `${left}px`;
  f.style.top = `${top}px`;
  // A transformed ancestor would move "fixed" off the viewport; correct by what it moved.
  const got = f.getBoundingClientRect();
  if (Math.abs(got.left - left) > 0.5 || Math.abs(got.top - top) > 0.5) {
    f.style.left = `${left - (got.left - left)}px`;
    f.style.top = `${top - (got.top - top)}px`;
  }
  f.dataset.side = goUp ? "top" : "bottom";
  f.style.visibility = "visible";
}

/** A layer above everything (see `placeNear`), following its anchor while the page scrolls
 * or the window or its own content changes size. */
export const Floating = forwardRef<HTMLDivElement, {
    anchor: RefObject<HTMLElement | null>;
    align?: "start" | "end";
    up?: boolean;
    className?: string;
    children: ReactNode;
    onClick?: (e: React.MouseEvent) => void;
    onMouseDown?: (e: React.MouseEvent) => void;
  }>(
  function Floating({ anchor, align = "start", up = false, className, children, onClick, onMouseDown }, outer) {
    const el = useRef<HTMLDivElement>(null);
    useImperativeHandle(outer, () => el.current!, []);
    const place = useCallback(() => {
      const a = anchor.current;
      const f = el.current;
      if (a && f) placeNear(a.getBoundingClientRect(), f, align, up);
    }, [anchor, align, up]);
    useLayoutEffect(() => {
      place();
      let frame = 0;
      const again = () => {
        cancelAnimationFrame(frame);
        frame = requestAnimationFrame(place);
      };
      window.addEventListener("scroll", again, true);
      window.addEventListener("resize", again);
      const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(again) : null;
      if (el.current) ro?.observe(el.current.firstElementChild ?? el.current);
      return () => {
        cancelAnimationFrame(frame);
        window.removeEventListener("scroll", again, true);
        window.removeEventListener("resize", again);
        ro?.disconnect();
      };
    }, [place]);
    return createPortal(
      <div ref={el} className={"floating" + (className ? ` ${className}` : "")} style={{ position: "fixed", top: 0, left: 0, visibility: "hidden" }} onClick={onClick} onMouseDown={onMouseDown}>
        {children}
      </div>,
      layerHost(anchor.current),
    );
  },
);
