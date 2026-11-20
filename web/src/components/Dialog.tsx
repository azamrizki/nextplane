import { useEffect, useRef, type ReactNode } from "react";
import { useT } from "../i18n";

/** A modal built on <dialog>: Esc and the backdrop close it, focus stays inside. */
export function Dialog({
  open,
  onClose,
  title,
  children,
  footer,
  width = 440,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const t = useT();
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal?.();
    if (!open && d.open) d.close?.();
  }, [open]);
  if (!open) return null;
  return (
    <dialog
      ref={ref}
      className="dialog"
      style={{ width }}
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      onClick={(e) => {
        if (e.target === ref.current) onClose();
      }}
      aria-label={title}
    >
      <div className="dialog-head">
        <h2>{title}</h2>
        <button className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
          ✕
        </button>
      </div>
      <div className="dialog-body">{children}</div>
      {footer ? <div className="dialog-foot">{footer}</div> : null}
    </dialog>
  );
}

/** Confirmation with an optional "type this to confirm" guard. */
export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  body,
  confirmLabel,
  danger,
  typeToConfirm,
  busy,
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: string;
  body: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  typeToConfirm?: { label: string; value: string };
  busy?: boolean;
}) {
  const t = useT();
  const inputRef = useRef<HTMLInputElement>(null);
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={title}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            className={danger ? "btn btn-danger" : "btn btn-primary"}
            disabled={busy}
            onClick={() => {
              if (typeToConfirm && inputRef.current?.value.trim() !== typeToConfirm.value) {
                inputRef.current?.focus();
                return;
              }
              onConfirm();
            }}
          >
            {confirmLabel}
          </button>
        </>
      }
    >
      <div className="form">
        <div className="muted">{body}</div>
        {typeToConfirm ? (
          <div className="field">
            <label htmlFor="confirm-input">{typeToConfirm.label}</label>
            <input id="confirm-input" ref={inputRef} className="input" autoComplete="off" />
          </div>
        ) : null}
      </div>
    </Dialog>
  );
}
