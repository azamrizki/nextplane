import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link, NavLink, Outlet, useNavigate, useParams } from "react-router";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { post } from "../api/client";
import type { MyWorkspace } from "../api/types";
import { useT } from "../i18n";
import { can } from "../lib/perm";
import { rememberWorkspace, useWorkspaces } from "../lib/queries";
import { useProjects } from "../lib/projects";
import { ProjectIcon } from "../pages/Projects";
import { useMe } from "../lib/session";
import { useRealtime } from "../lib/realtime";
import { CommandPalette } from "./CommandPalette";
import { useUnreadCount } from "../pages/Inbox";
import { Avatar, WsMark } from "./Avatar";
import { FullPageSpinner } from "./ui";

/** The app frame for a workspace: sidebar (switcher, nav, user) and the routed page. */
export function WorkspaceLayout() {
  const { slug = "" } = useParams();
  const list = useWorkspaces();
  const navigate = useNavigate();
  const ws = list.data?.find((w) => w.slug === slug);
  useRealtime(ws ? ws.slug : "");

  useEffect(() => {
    if (ws) rememberWorkspace(ws.slug);
  }, [ws]);
  useEffect(() => {
    if (list.isSuccess && !ws) navigate("/", { replace: true });
  }, [list.isSuccess, ws, navigate]);

  if (list.isLoading || !ws) return <FullPageSpinner />;
  return (
    <div className="app">
      <Sidebar ws={ws} all={list.data ?? []} />
      <main className="app-main">
        <Outlet context={ws} />
      </main>
      <CommandPalette ws={ws} />
    </div>
  );
}

function Sidebar({ ws, all }: { ws: MyWorkspace; all: MyWorkspace[] }) {
  const t = useT();
  const unread = useUnreadCount(ws.slug);
  const base = `/${ws.slug}`;
  return (
    <aside className="sidebar">
      <WorkspaceSwitcher ws={ws} all={all} />
      <nav className="side-nav">
        <NavLink to={base} end className="side-link">
          {t("nav.home")}
        </NavLink>
        <NavLink to={`${base}/inbox`} className="side-link">
          {t("nav.inbox")}
          {unread.data ? <span className="side-count" aria-label={t("inbox.unreadCount", { n: unread.data })}>{unread.data}</span> : null}
        </NavLink>
        <NavLink to={`${base}/my-work`} className="side-link">
          {t("nav.myWork")}
        </NavLink>
        <NavLink to={`${base}/drafts`} className="side-link">
          {t("nav.drafts")}
        </NavLink>
        {ws.role !== "guest" ? (
          <NavLink to={`${base}/analytics`} className="side-link">
            {t("nav.analytics")}
          </NavLink>
        ) : null}
        {can.viewMembers(ws) ? (
          <NavLink to={`${base}/settings/members`} className="side-link">
            {t("nav.members")}
          </NavLink>
        ) : null}
        <NavLink to={`${base}/settings`} end className="side-link">
          {t("nav.settings")}
        </NavLink>
      </nav>
      <SideProjects slug={ws.slug} />
      <div style={{ flex: 1 }} />
      <UserBlock />
    </aside>
  );
}

function SideProjects({ slug }: { slug: string }) {
  const t = useT();
  const projects = useProjects(slug);
  const mine = (projects.data ?? []).filter((p) => p.is_member);
  return (
    <nav className="side-nav" aria-label={t("nav.projects")}>
      <div className="side-section">
        <NavLink to={`/${slug}/projects`} end className="side-section-link">
          {t("nav.projects")}
        </NavLink>
      </div>
      {mine.map((p) => (
        <NavLink key={p.id} to={`/${slug}/projects/${p.id}`} className="side-link">
          <ProjectIcon p={p} size={18} />
          <span className="ellipsis">{p.name}</span>
        </NavLink>
      ))}
    </nav>
  );
}

function WorkspaceSwitcher({ ws, all }: { ws: MyWorkspace; all: MyWorkspace[] }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <Popover
      open={open}
      onClose={() => setOpen(false)}
      trigger={
        <button className="ws-switch" onClick={() => setOpen((o) => !o)} aria-haspopup="menu" aria-expanded={open}>
          <WsMark name={ws.name} logo={ws.logo} />
          <span className="ws-switch-name">{ws.name}</span>
          <span className="faint">▾</span>
        </button>
      }
    >
      <div className="menu" role="menu">
        <div className="menu-label">{t("nav.workspaces")}</div>
        {all.map((w) => (
          <Link key={w.id} to={`/${w.slug}`} className={"menu-item" + (w.id === ws.id ? " active" : "")} role="menuitem" onClick={() => setOpen(false)}>
            <WsMark name={w.name} logo={w.logo} size={18} />
            <span className="grow">{w.name}</span>
            <span className="faint small">{t(`role.${w.role}`)}</span>
          </Link>
        ))}
        <div className="menu-sep" />
        <Link to="/create-workspace" className="menu-item" role="menuitem" onClick={() => setOpen(false)}>
          + {t("nav.createWorkspace")}
        </Link>
      </div>
    </Popover>
  );
}

function UserBlock() {
  const t = useT();
  const me = useMe().data!;
  const [open, setOpen] = useState(false);
  const qc = useQueryClient();
  const navigate = useNavigate();
  const logout = useMutation({
    mutationFn: () => post<void>("/auth/logout"),
    onSuccess: () => {
      qc.clear();
      navigate("/login", { replace: true });
    },
  });
  return (
    <Popover
      open={open}
      onClose={() => setOpen(false)}
      up
      trigger={
        <button className="user-block" onClick={() => setOpen((o) => !o)} aria-haspopup="menu" aria-expanded={open}>
          <Avatar name={me.name} email={me.email} src={me.avatar} />
          <span className="grow ellipsis">{me.name || me.email}</span>
        </button>
      }
    >
      <div className="menu" role="menu">
        <div className="menu-label ellipsis">{me.email}</div>
        <Link to="/profile" className="menu-item" role="menuitem" onClick={() => setOpen(false)}>
          {t("nav.profile")}
        </Link>
        {me.is_instance_admin ? (
          <Link to="/admin" className="menu-item" role="menuitem" onClick={() => setOpen(false)}>
            {t("nav.admin")}
          </Link>
        ) : null}
        <div className="menu-sep" />
        <button className="menu-item" role="menuitem" onClick={() => logout.mutate()}>
          {t("shell.logout")}
        </button>
      </div>
    </Popover>
  );
}

/** A trigger plus a floating panel that closes on outside click and Esc. */
export function Popover({ trigger, children, open, onClose, up }: { trigger: ReactNode; children: ReactNode; open: boolean; onClose: () => void; up?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, onClose]);
  return (
    <div className="popover-wrap" ref={ref}>
      {trigger}
      {open ? <div className={"popover" + (up ? " popover-up" : "")}>{children}</div> : null}
    </div>
  );
}

/** Simple centered page frame for pages outside a workspace (profile, admin, create). */
export function PlainPage({ children, back }: { children: ReactNode; back?: boolean }) {
  const t = useT();
  return (
    <div className="plain-page">
      {back ? (
        <Link to="/" className="small">
          ← {t("common.back")}
        </Link>
      ) : null}
      {children}
    </div>
  );
}
