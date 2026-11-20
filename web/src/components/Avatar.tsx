const COLORS = ["#0e7c6b", "#6e56cf", "#d6409f", "#e5484d", "#f76b15", "#ad7f58", "#3e63dd", "#12a594"];

function hash(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) | 0;
  return Math.abs(h);
}

export function initials(name: string, email = ""): string {
  const src = name.trim() || email;
  const parts = src.split(/[\s@._-]+/).filter(Boolean);
  const a = parts[0]?.[0] ?? "?";
  const b = parts.length > 1 ? parts[1]![0] : "";
  return (a + b).toUpperCase();
}

export function Avatar({ name, email, src, size = 24 }: { name: string; email?: string; src?: string | null; size?: number }) {
  const style = { width: size, height: size, fontSize: Math.round(size * 0.42) };
  if (src) return <img className="avatar" src={src} alt="" style={style} />;
  return (
    <span className="avatar" style={{ ...style, background: COLORS[hash(email || name) % COLORS.length] }} aria-hidden>
      {initials(name, email)}
    </span>
  );
}

/** A workspace mark: its logo, or its first letter on a colored square. */
export function WsMark({ name, logo, size = 22 }: { name: string; logo?: string | null; size?: number }) {
  const style = { width: size, height: size, fontSize: Math.round(size * 0.5) };
  if (logo) return <img className="ws-mark" src={logo} alt="" style={style} />;
  return (
    <span className="ws-mark" style={{ ...style, background: COLORS[hash(name) % COLORS.length] }} aria-hidden>
      {(name.trim()[0] ?? "?").toUpperCase()}
    </span>
  );
}
