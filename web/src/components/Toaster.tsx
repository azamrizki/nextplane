import { CircleAlert, CircleCheck, Info, X } from "lucide-react";
import { Link } from "react-router";
import { useI18n } from "../i18n";
import { dismiss, useToasts } from "../lib/toast";

/** Toasts at the bottom right, newest last; a polite live region for screen readers. */
export function Toaster() {
  const { t } = useI18n();
  const list = useToasts();
  return (
    <div className="toasts" role="status" aria-live="polite">
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
