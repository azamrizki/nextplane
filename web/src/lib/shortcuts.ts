import { useEffect, useRef } from "react";
import { useNavigate, useParams } from "react-router";
import type { MyWorkspace } from "../api/types";
import { emitShell } from "./shell";

/** Keyboard shortcuts as Plane has them: Ctrl/⌘ combos, single keys and two-key sequences
 * ("G then I"). Single keys and sequences are ignored while typing or inside a dialog/menu. */
export const SEQUENCES: { keys: string; action: string; project?: boolean }[] = [
  { keys: "g h", action: "home" },
  { keys: "g x", action: "inbox" },
  { keys: "g y", action: "myWork" },
  { keys: "g p", action: "projects" },
  { keys: "g v", action: "views" },
  { keys: "g a", action: "analytics" },
  { keys: "g j", action: "drafts" },
  { keys: "g s", action: "settings" },
  { keys: "g i", action: "items", project: true },
  { keys: "g c", action: "cycles", project: true },
  { keys: "g m", action: "modules", project: true },
  { keys: "g d", action: "pages", project: true },
  { keys: "g k", action: "intake", project: true },
  { keys: "n i", action: "newItem" },
  { keys: "n p", action: "newProject" },
];

export function isTyping(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return !!el?.closest?.("input, textarea, select, [contenteditable=true], [role=dialog], [role=menu], [role=listbox], dialog");
}

export function useShortcuts(ws: MyWorkspace | undefined, toggleSidebar: () => void) {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const pending = useRef<{ key: string; at: number } | null>(null);
  useEffect(() => {
    if (!ws) return;
    const base = `/${ws.slug}`;
    const proj = projectId ? `${base}/projects/${projectId}` : null;
    const run = (action: string) => {
      const to: Record<string, string | null> = {
        home: base,
        inbox: `${base}/inbox`,
        myWork: `${base}/my-work`,
        projects: `${base}/projects`,
        views: `${base}/views/all`,
        analytics: `${base}/analytics`,
        drafts: `${base}/drafts`,
        settings: proj ? `${proj}/settings` : `${base}/settings`,
        items: proj,
        cycles: proj && `${proj}/cycles`,
        modules: proj && `${proj}/modules`,
        pages: proj && `${proj}/pages`,
        intake: proj && `${proj}/intake`,
        newProject: `${base}/projects?new=1`,
      };
      if (action === "newItem") return emitShell("create-item");
      const dest = to[action];
      if (dest) navigate(dest);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat) return;
      const mod = e.metaKey || e.ctrlKey;
      if (mod && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "b") {
        e.preventDefault();
        toggleSidebar();
        return;
      }
      if (mod && e.key === "/") {
        e.preventDefault();
        emitShell("shortcuts");
        return;
      }
      if (mod || e.altKey) {
        pending.current = null;
        return;
      }
      if (isTyping(e.target)) return;
      const k = e.key.toLowerCase();
      const now = Date.now();
      const first = pending.current && now - pending.current.at < 1000 ? pending.current.key : null;
      if (first) {
        const hit = SEQUENCES.find((s) => s.keys === `${first} ${k}` && (!s.project || projectId));
        pending.current = null;
        if (hit) {
          e.preventDefault();
          run(hit.action);
          return;
        }
      }
      if (SEQUENCES.some((s) => s.keys.startsWith(`${k} `))) {
        pending.current = { key: k, at: now };
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [ws, projectId, navigate, toggleSidebar]);
}
