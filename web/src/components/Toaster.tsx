import { CircleAlert, CircleCheck, Info, X } from "lucide-react";
import { useLayoutEffect, useRef } from "react";
import { Link } from "react-router";
import { useI18n } from "../i18n";
import { dismiss, useToasts } from "../lib/toast";

/** Toasts at the bottom right, newest last; a polite live region for screen readers. The list
 * is a manual popover, so it sits in the browser's top layer: above modal dialogs too (a toast
 * raised from inside a dialog is not hidden behind its backdrop). */
export function Toaster() {
  const { t } = useI18n();
  const list = useToasts();
  const ref = useRef<HTMLDivElement>(null);
  const newest = list[list.length - 1]?.id;
  useLayoutEffect(() => {
    const el = ref.current as (HTMLDivElement & { showPopover?: () => void; hidePopover?: () => void }) | null;
    if (!el?.showPopover) return;
    const shown = el.matches(":popover-open");
    // Showing again moves it above whatever entered the top layer since (a dialog).
    if (shown) el.hidePopover?.();
    if (list.length) el.showPopover();
  }, [newest, list.length]);
  return (
    <div ref={ref} className="toasts" role="status" aria-live="polite" popover="manual">
      {list.map((x) => (
        <div key={x.id} className={`toast toast-${x.kind}`}>
          {x.kind === "success" ? <CircleCheck aria-hidden /> : x.kind === "error" ? <CircleAlert aria-hidden /> : <Info aria-hidden />}
          <div className="toast-text">
            <b>{x.title}</b>
            {x.body ? <span>{x.body}</span> : null}
            {x.link ? (
              <Link to={x.link.to} onClick={() => dismiss(x.id)}>
                {x.link.label}
              </Link>
            ) : null}
          </div>
          <button className="icon-btn icon-btn-sm" aria-label={t("common.close")} onClick={() => dismiss(x.id)}>
            <X />
          </button>
        </div>
      ))}
    </div>
  );
}
