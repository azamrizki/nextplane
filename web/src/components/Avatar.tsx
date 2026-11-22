// Flat, muted colours for people and workspace marks (white initials on each pass 4.5:1).
const COLORS = ["#2f7d72", "#625ab8", "#9c5274", "#ad4c46", "#b25f3d", "#7d6548", "#3769ab", "#2b7a86"];

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
