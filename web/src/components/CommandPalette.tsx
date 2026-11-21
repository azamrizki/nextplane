import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useNavigate, useParams } from "react-router";
import { get } from "../api/client";
import type { MyWorkspace } from "../api/types";
import { useI18n } from "../i18n";
import { useProjects } from "../lib/projects";
import { useWorkspaces } from "../lib/queries";
import { useTheme } from "../lib/theme";
import { Dialog } from "./Dialog";
import { emitShell, useShellEvent } from "../lib/shell";
import { SEQUENCES } from "../lib/shortcuts";
import { WsMark } from "./Avatar";
import { Briefcase, Keyboard, ListTodo, Plus, Search, Settings, SunMoon, UserRound, UsersRound } from "lucide-react";

interface Hit {
  kind: string;
  work_item_id: string;
  project_id: string;
  identifier: string;
  sequence: number;
  title: string;
  snippet: string;
}

interface Entry {
  id: string;
  section: string;
  label: string;
  hint?: string;
  icon?: ReactNode;
  run: () => void;
}

/** Ctrl/⌘K: search work items and projects, jump anywhere, run common actions. `?` lists
 * the keyboard shortcuts. */
export function CommandPalette({ ws }: { ws: MyWorkspace }) {
  const { t } = useI18n();
  const navigate = useNavigate();
  const { projectId } = useParams();
  const [open, setOpen] = useState(false);
  const [help, setHelp] = useState(false);
  const [q, setQ] = useState("");
  const [debounced, setDebounced] = useState("");
  const [index, setIndex] = useState(0);
  const [theme, setTheme] = useTheme();
  const projects = useProjects(ws.slug);
  const workspaces = useWorkspaces();
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setQ("");
        setOpen((o) => !o);
      }
      const typing = (e.target as HTMLElement).closest("input, textarea, [contenteditable=true], select");
      if (!typing && e.key === "?" && !open) {
        e.preventDefault();
        setHelp(true);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open]);
  useShellEvent("palette", useCallback(() => { setQ(""); setOpen(true); }, []));
  useShellEvent("shortcuts", useCallback(() => setHelp(true), []));
  useEffect(() => {
    const h = setTimeout(() => setDebounced(q.trim()), 150);
    return () => clearTimeout(h);
  }, [q]);
  const search = useQuery({
    queryKey: ["search", ws.slug, debounced],
    queryFn: () => get<{ projects: unknown[]; work_items: Hit[] }>(`/workspaces/${ws.slug}/search?q=${encodeURIComponent(debounced)}&limit=10`),
    enabled: open && debounced.length >= 2,
  });

  const go = (to: string) => {
    setOpen(false);
    navigate(to);
  };
  const entries = useMemo<Entry[]>(() => {
    const ql = q.trim().toLowerCase();
    const match = (s: string) => !ql || s.toLowerCase().includes(ql);
    const out: Entry[] = [];
    const actions: Entry[] = [
      { id: "new", section: t("pal.actions"), label: t("pal.newItem"), hint: "N I", icon: <Plus />, run: () => { setOpen(false); emitShell("create-item"); } },
      { id: "projects", section: t("pal.actions"), label: t("pal.goProjects"), hint: "G P", icon: <Briefcase />, run: () => go(`/${ws.slug}/projects`) },
      { id: "members", section: t("pal.actions"), label: t("pal.goMembers"), icon: <UsersRound />, run: () => go(`/${ws.slug}/settings/members`) },
      { id: "settings", section: t("pal.actions"), label: t("pal.goSettings"), hint: "G S", icon: <Settings />, run: () => go(`/${ws.slug}/settings`) },
      { id: "profile", section: t("pal.actions"), label: t("pal.goProfile"), icon: <UserRound />, run: () => go(`/profile`) },
      { id: "theme", section: t("pal.actions"), label: t("pal.toggleTheme"), icon: <SunMoon />, run: () => { setTheme(document.documentElement.dataset.theme === "dark" ? "light" : "dark"); setOpen(false); } },
      { id: "keys", section: t("pal.actions"), label: t("pal.shortcuts"), hint: "?", icon: <Keyboard />, run: () => { setOpen(false); setHelp(true); } },
    ];
    out.push(...actions.filter((a) => match(a.label)));
    for (const p of projects.data ?? []) {
      if (match(p.name) || match(p.identifier)) {
        out.push({ id: `p:${p.id}`, section: t("pal.projects"), label: p.name, hint: p.identifier, icon: <span>{p.icon ?? "▢"}</span>, run: () => go(`/${ws.slug}/projects/${p.id}`) });
      }
    }
    if (debounced.length >= 2) {
      for (const h of search.data?.work_items ?? []) {
        out.push({
          id: `w:${h.work_item_id}`,
          section: t("pal.items"),
          icon: <ListTodo />,
          label: h.title,
          hint: `${h.identifier}-${h.sequence}`,
          run: () => go(`/${ws.slug}/projects/${h.project_id}/work-items/${h.work_item_id}`),
        });
      }
    }
    for (const w of workspaces.data ?? []) {
      if (w.id !== ws.id && match(w.name)) {
        out.push({ id: `ws:${w.id}`, section: t("pal.workspaces"), label: w.name, icon: <WsMark name={w.name} logo={w.logo} size={16} />, run: () => go(`/${w.slug}`) });
      }
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [q, debounced, search.data, projects.data, workspaces.data, projectId, ws, theme, t]);
  useEffect(() => setIndex(0), [q, debounced]);
  useEffect(() => {
    listRef.current?.querySelector(".focused")?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setIndex((i) => Math.min(i + 1, entries.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      entries[index]?.run();
    }
  };

  let lastSection = "";
  return (
    <>
      <Dialog open={open} onClose={() => setOpen(false)} title={t("pal.placeholder")} width={672} bare>
        <div className="palette" onKeyDown={onKey}>
          <div className="palette-search">
            <Search aria-hidden />
            <input className="palette-input" autoFocus placeholder={t("pal.placeholder")} aria-label={t("pal.placeholder")} value={q} onChange={(e) => setQ(e.target.value)} role="combobox" aria-expanded="true" aria-controls="palette-list" />
          </div>
          <div className="palette-list" id="palette-list" role="listbox" ref={listRef}>
            {entries.map((e, i) => {
              const head = e.section !== lastSection ? <div className="menu-label">{e.section}</div> : null;
              lastSection = e.section;
              return (
                <div key={e.id}>
                  {head}
                  <button type="button" role="option" aria-selected={i === index} className={"menu-item" + (i === index ? " focused" : "")} onMouseEnter={() => setIndex(i)} onClick={e.run}>
                    {e.icon ?? null}
                    <span className="grow ellipsis">{e.label}</span>
                    {e.hint ? <kbd className="keycap">{e.hint}</kbd> : null}
                  </button>
                </div>
              );
            })}
          </div>
          <div className="palette-foot">{t("pal.hint")}</div>
        </div>
      </Dialog>
      <Dialog open={help} onClose={() => setHelp(false)} title={t("keys.title")}>
        <dl className="keys">
          {[
            ["Ctrl/⌘ K", t("keys.palette")],
            ["Ctrl/⌘ B", t("keys.sidebar")],
            ["Ctrl/⌘ /", t("keys.help")],
            ["C", t("keys.create")],
            ["Esc", t("keys.close")],
            ["X", t("keys.select")],
            ["Ctrl/⌘ Enter", t("keys.submit")],
            ...SEQUENCES.map((q) => [q.keys.toUpperCase().replace(" ", ` ${t("keys.then")} `), t(`keys.seq.${q.action}` as "keys.seq.home")]),
          ].map(([k, d]) => (
            <div key={k} className="keys-row">
              <dt>{k!.split(` ${t("keys.then")} `).map((part, i) => (
                <span key={i}>{i ? <span className="faint small"> {t("keys.then")} </span> : null}<kbd className="keycap">{part}</kbd></span>
              ))}</dt>
              <dd>{d}</dd>
            </div>
          ))}
        </dl>
      </Dialog>
    </>
  );
}
