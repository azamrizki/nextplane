import { CloudUpload } from "lucide-react";
import { useState } from "react";

/** A dashed glass area to drop a file on, or click to browse (the input stays a real file input,
 * labelled by `label`). */
export function FileDrop({ label, hint, accept, onFile, disabled }: { label: string; hint?: string; accept?: string; onFile: (f: File) => void; disabled?: boolean }) {
  const [over, setOver] = useState(false);
  return (
    <label
      className={"file-drop" + (over ? " over" : "") + (disabled ? " disabled" : "")}
      onDragOver={(e) => {
        if (disabled) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        const f = e.dataTransfer.files?.[0];
        if (f && !disabled) onFile(f);
      }}
    >
      <span className="file-drop-icon" aria-hidden>
        <CloudUpload />
      </span>
      <b>{label}</b>
      {hint ? <span className="faint small">{hint}</span> : null}
      <input
        type="file"
        accept={accept}
        hidden
        disabled={disabled}
        aria-label={label}
        onChange={(e) => {
          const f = e.target.files?.[0];
          e.target.value = "";
          if (f) onFile(f);
        }}
      />
    </label>
  );
}
