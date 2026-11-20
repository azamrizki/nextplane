import type { InputHTMLAttributes, ReactNode } from "react";
import { useId } from "react";

export function Logo() {
  return (
    <span className="logo">
      <img src="/icon.svg" alt="" />
      NextPlane
    </span>
  );
}

export function Spinner({ label }: { label?: string }) {
  return <span className="spinner" role="status" aria-label={label ?? "loading"} />;
}

export function FullPageSpinner() {
  return (
    <div className="center-fill">
      <Spinner />
    </div>
  );
}

type FieldProps = InputHTMLAttributes<HTMLInputElement> & { label: string; hint?: ReactNode };

export function TextField({ label, hint, ...input }: FieldProps) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <input id={id} className="input" {...input} />
      {hint ? <span className="hint">{hint}</span> : null}
    </div>
  );
}

export function ErrorAlert({ message }: { message: string | null | undefined }) {
  if (!message) return null;
  return (
    <div className="alert alert-error" role="alert">
      {message}
    </div>
  );
}
