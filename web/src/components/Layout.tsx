import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Boxes,
  Briefcase,
  ChartColumn,
  Check,
  ChevronDown,
  ChevronRight,
  CircleHelp,
  CirclePlus,
  CircleUserRound,
  Contrast,
  Ellipsis,
  FilePen,
  FileText,
  House,
  Archive,
  Import,
  Inbox,
  Keyboard,
  Link as LinkIcon,
  ListTodo,
  LogOut,
  PanelLeft,
  Plus,
  Search,
  Settings,
  Share2,
  ShieldCheck,
  SquarePen,
  UserPlus,
  UserRound,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Link, NavLink, Outlet, useLocation, useNavigate, useParams } from "react-router";
import { get, post } from "../api/client";
import type { MyWorkspace, Project } from "../api/types";
import { useI18n, useT } from "../i18n";
import { can, pcan } from "../lib/perm";
import { rememberWorkspace, useWorkspaces } from "../lib/queries";
import { useProjects } from "../lib/projects";
import { ShellContext, emitShell, useShell, useShellState } from "../lib/shell";
import { useIntakeCount } from "../lib/intake";
import { ProjectIcon } from "../pages/Projects";
import { useMe } from "../lib/session";
import { useRealtime } from "../lib/realtime";
import { useShortcuts } from "../lib/shortcuts";
import { CommandPalette } from "./CommandPalette";
import { GlobalCreate } from "./CreateItem";
import { Toaster } from "./Toaster";
import { useUnreadCount } from "../pages/Inbox";
import { Avatar, WsMark } from "./Avatar";
import { ConfirmDialog } from "./Dialog";
import { FullPageSpinner } from "./ui";

/** The app frame for a workspace: a top bar on the canvas, then one rounded panel holding the
 * sidebar and the routed page (Plane's layout). */
export function WorkspaceLayout() {
  const { slug = "" } = useParams();
  const list = useWorkspaces();
  const navigate = useNavigate();
  const ws = list.data?.find((w) => w.slug === slug);
  const shell = useShellState();
  useRealtime(ws ? ws.slug : "");
  useShortcuts(ws, shell.toggle);

  useEffect(() => {
    if (ws) rememberWorkspace(ws.slug);
  }, [ws]);
  useEffect(() => {
    if (list.isSuccess && !ws) navigate("/", { replace: true });
  }, [list.isSuccess, ws, navigate]);

  if (list.isLoading || !ws) return <FullPageSpinner />;
  return (
    <ShellContext.Provider value={shell}>
      <div className={"app" + (shell.collapsed ? " sidebar-collapsed" : "")}>
        <TopBar ws={ws} all={list.data ?? []} />
        <div className="app-body">
          <div className="app-panel">
            <Sidebar ws={ws} />
            {!shell.collapsed ? <div className="sidebar-scrim" onClick={shell.toggle} aria-hidden /> : null}
            <main className="app-main">
              <Outlet context={ws} />
            </main>
          </div>
        </div>
        <CommandPalette ws={ws} />
        <GlobalCreate ws={ws} />
        <Toaster />
      </div>
    </ShellContext.Provider>
  );
}

function TopBar({ ws, all }: { ws: MyWorkspace; all: MyWorkspace[] }) {
  const t = useT();
  const unread = useUnreadCount(ws.slug);
  const { pathname } = useLocation();
  const inInbox = pathname.startsWith(`/${ws.slug}/inbox`);
  return (
    <header className="topbar">
      <div className="topbar-left">
        <WorkspaceSwitcher ws={ws} all={all} />
      </div>
      <button className="top-search" onClick={() => emitShell("palette")} aria-label={t("nav.search")}>
        <Search aria-hidden />
        <span className="grow">{t("nav.search")}</span>
        <kbd className="keycap">Ctrl K</kbd>
      </button>
      <div className="topbar-right">
        <Link
          to={`/${ws.slug}/inbox`}
          className={"icon-btn topbar-btn" + (inInbox ? " on" : "")}
          aria-label={unread.data ? `${t("nav.inbox")} ${t("inbox.unreadCount", { n: unread.data })}` : t("nav.inbox")}
          title={t("nav.inbox")}
        >
          <Inbox />
          {unread.data ? <span className="topbar-dot" aria-hidden /> : null}
        </Link>
        <HelpMenu />
        <UserMenu />
      </div>
    </header>
  );
}

function Sidebar({ ws }: { ws: MyWorkspace }) {
  const { t } = useI18n();
  const shell = useShell();
  const base = `/${ws.slug}`;
  const [wsOpen, setWsOpen] = useState(() => localStorage.getItem("np.side.workspace") !== "0");
  const version = useQuery({ queryKey: ["version"], queryFn: () => get<{ version: string }>("/version"), staleTime: Infinity });
  const closeIfNarrow = () => {
    if (window.matchMedia?.("(max-width: 767px)").matches) shell.toggle();
  };
  return (
    <aside className="sidebar" aria-label={t("nav.sidebar")} inert={shell.collapsed}>
      <div className="side-head">
        <span className="side-title">{t("nav.projects")}</span>
        <button className="icon-btn" onClick={shell.toggle} aria-label={t("nav.collapse")} title={`${t("nav.collapse")} (Ctrl/⌘ B)`}>
          <PanelLeft />
        </button>
      </div>
      <div className="side-quick">
        <button className="btn btn-lg btn-block side-new" onClick={() => emitShell("create-item")} disabled={ws.role === "guest"}>
          <SquarePen />
          <span className="ellipsis">{t("nav.newItem")}</span>
        </button>
      </div>
      <div className="side-scroll" onClick={(e) => (e.target as HTMLElement).closest("a") && closeIfNarrow()}>
        <nav className="side-nav">
          <NavLink to={base} end className="side-link">
            <House />
            <span className="ellipsis">{t("nav.home")}</span>
          </NavLink>
          <NavLink to={`${base}/my-work`} className="side-link">
            <CircleUserRound />
            <span className="ellipsis">{t("nav.myWork")}</span>
          </NavLink>
          <NavLink to={`${base}/drafts`} className="side-link">
            <FilePen />
            <span className="ellipsis">{t("nav.drafts")}</span>
          </NavLink>
        </nav>
        <section className="side-section">
          <button
            className="side-section-head"
            aria-expanded={wsOpen}
            onClick={() => {
              localStorage.setItem("np.side.workspace", wsOpen ? "0" : "1");
              setWsOpen(!wsOpen);
            }}
          >
            <span>{t("nav.workspaceSection")}</span>
            <ChevronDown className={"side-caret" + (wsOpen ? "" : " closed")} aria-hidden />
          </button>
          {wsOpen ? (
            <nav className="side-nav">
              <NavLink to={`${base}/projects`} end className="side-link">
                <Briefcase />
                <span className="ellipsis">{t("nav.projects")}</span>
              </NavLink>
              {ws.role !== "guest" ? (
                <NavLink to={`${base}/analytics`} className="side-link">
                  <ChartColumn />
                  <span className="ellipsis">{t("nav.analytics")}</span>
                </NavLink>
              ) : null}
            </nav>
          ) : null}
        </section>
        <SideProjects ws={ws} />
      </div>
      <div className="side-foot">
        <span className="badge badge-neutral" title={t("nav.version")}>
          NextPlane {version.data ? `v${version.data.version}` : ""}
        </span>
      </div>
    </aside>
  );
}

function SideProjects({ ws }: { ws: MyWorkspace }) {
  const { t } = useI18n();
  const projects = useProjects(ws.slug);
  const { projectId } = useParams();
  const [open, setOpen] = useState(true);
  const mine = (projects.data ?? []).filter((p) => p.is_member && !p.archived_at);
  return (
    <section className="side-section">
      <div className="side-section-row">
        <button className="side-section-head" aria-expanded={open} onClick={() => setOpen(!open)}>
          <span>{t("nav.projects")}</span>
          <ChevronDown className={"side-caret" + (open ? "" : " closed")} aria-hidden />
        </button>
        {can.createProject(ws) ? (
          <Link to={`/${ws.slug}/projects?new=1`} className="icon-btn icon-btn-xs side-section-add" aria-label={t("nav.createProject")} title={t("nav.createProject")}>
            <Plus />
          </Link>
        ) : null}
      </div>
      {open ? (
        <nav className="side-nav" aria-label={t("nav.projects")}>
          {mine.map((p) => (
            <SideProject key={p.id} ws={ws} p={p} current={p.id === projectId} />
          ))}
          {projects.isSuccess && mine.length === 0 ? (
            <Link to={`/${ws.slug}/projects`} className="side-link faint">
              <Plus />
              <span className="ellipsis">{t("nav.browseProjects")}</span>
            </Link>
          ) : null}
        </nav>
      ) : null}
    </section>
  );
}

function SideProject({ ws, p, current }: { ws: MyWorkspace; p: Project; current: boolean }) {
  const { t } = useI18n();
  const key = `np.side.project.${p.id}`;
  const [expanded, setExpanded] = useState(() => current || localStorage.getItem(key) === "1");
  const [menu, setMenu] = useState(false);
  const row = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (current) {
      setExpanded(true);
      const h = setTimeout(() => row.current?.scrollIntoView({ block: "nearest", behavior: "smooth" }), 200);
      return () => clearTimeout(h);
    }
  }, [current]);
  const toggle = () => {
    localStorage.setItem(key, expanded ? "0" : "1");
    setExpanded(!expanded);
  };
  const base = `/${ws.slug}/projects/${p.id}`;
  const intake = useIntakeCount(ws.slug, p.id, p.feature_intake && expanded);
  const open = intake.data?.open ?? 0;
  return (
    <div className="side-project">
      <div className={"side-link side-project-row" + (menu ? " menu-open" : "")} ref={row}>
        <button className="side-project-toggle" onClick={toggle} aria-expanded={expanded}>
          <ProjectIcon p={p} size={16} />
          <span className="ellipsis grow">{p.name}</span>
        </button>
        <ProjectMenu ws={ws} p={p} open={menu} setOpen={setMenu} />
        <button className="icon-btn icon-btn-xs side-row-action" onClick={toggle} aria-label={expanded ? t("nav.collapseProject") : t("nav.expandProject")} tabIndex={-1}>
          <ChevronRight className={"side-chevron" + (expanded ? " open" : "")} />
        </button>
      </div>
      {expanded ? (
        <div className="side-children">
          <NavLink to={base} end className="side-link side-child">
            <ListTodo />
            <span className="ellipsis">{t("ptab.items")}</span>
          </NavLink>
          {p.feature_cycles ? (
            <NavLink to={`${base}/cycles`} className="side-link side-child">
              <Contrast />
              <span className="ellipsis">{t("ptab.cycles")}</span>
            </NavLink>
          ) : null}
          {p.feature_modules ? (
            <NavLink to={`${base}/modules`} className="side-link side-child">
              <Boxes />
              <span className="ellipsis">{t("ptab.modules")}</span>
            </NavLink>
          ) : null}
          {p.feature_pages ? (
            <NavLink to={`${base}/pages`} className="side-link side-child">
              <FileText />
              <span className="ellipsis">{t("ptab.pages")}</span>
            </NavLink>
          ) : null}
          {p.feature_intake ? (
            <NavLink to={`${base}/intake`} className="side-link side-child">
              <Import />
              <span className="ellipsis">{t("ptab.intake")}</span>
              {open > 0 ? <span className="side-count" aria-label={`(${open})`}>{open}</span> : null}
            </NavLink>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function ProjectMenu({ ws, p, open, setOpen }: { ws: MyWorkspace; p: Project; open: boolean; setOpen: (o: boolean) => void }) {
  const { t } = useI18n();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [leaving, setLeaving] = useState(false);
  const base = `/${ws.slug}/projects/${p.id}`;
  const leave = useMutation({
    mutationFn: () => post<void>(`/workspaces/${ws.slug}/projects/${p.id}/leave`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["projects", ws.slug] });
      navigate(`/${ws.slug}/projects`);
    },
  });
  const close = useCallback(() => setOpen(false), [setOpen]);
  return (
    <>
      <Popover
        open={open}
        onClose={close}
        right
        trigger={
          <button className="icon-btn icon-btn-xs side-row-action" onClick={() => setOpen(!open)} aria-label={t("nav.projectMenu", { name: p.name })} aria-haspopup="menu" aria-expanded={open}>
            <Ellipsis />
          </button>
        }
      >
        <div className="menu" role="menu">
          {pcan.editSettings(p) ? (
            <Link to={`${base}/settings/publish`} className="menu-item" role="menuitem" onClick={close}>
              <Share2 />
              {t("nav.publish")}
            </Link>
          ) : null}
          <button
            className="menu-item"
            role="menuitem"
            onClick={() => {
              void navigator.clipboard?.writeText(`${location.origin}${base}`);
              close();
            }}
          >
            <LinkIcon />
            {t("nav.copyLink")}
          </button>
          {pcan.viewArchives(p) ? (
            <Link to={`${base}/archives`} className="menu-item" role="menuitem" onClick={close}>
              <Archive />
              {t("ptab.archives")}
            </Link>
          ) : null}
          <Link to={`${base}/settings`} className="menu-item" role="menuitem" onClick={close}>
            <Settings />
            {t("nav.settings")}
          </Link>
          {p.my_role && p.my_role !== "admin" ? (
            <button className="menu-item danger" role="menuitem" onClick={() => (close(), setLeaving(true))}>
              <LogOut />
              {t("proj.leave")}
            </button>
          ) : null}
        </div>
      </Popover>
      <ConfirmDialog
        open={leaving}
        onClose={() => setLeaving(false)}
        title={t("proj.leave")}
        body={t("nav.leaveConfirm", { name: p.name })}
        confirmLabel={t("proj.leave")}
        danger
        onConfirm={() => leave.mutate()}
      />
    </>
  );
}

function WorkspaceSwitcher({ ws, all }: { ws: MyWorkspace; all: MyWorkspace[] }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const others = all.filter((w) => w.id !== ws.id);
  return (
    <Popover
      open={open}
      onClose={close}
      trigger={
        <button className={"ws-switch" + (open ? " open" : "")} onClick={() => setOpen((o) => !o)} aria-haspopup="menu" aria-expanded={open} aria-label={t("nav.switchWorkspace")}>
          <WsMark name={ws.name} logo={ws.logo} size={24} />
          <span className="ws-switch-name">{ws.name}</span>
          <ChevronDown className="ws-switch-chevron" aria-hidden />
        </button>
      }
    >
      <div className="menu ws-menu" role="menu">
        <div className="ws-menu-current">
          <div className="ws-menu-row">
            <WsMark name={ws.name} logo={ws.logo} size={32} />
            <span className="grow">
              <span className="ws-menu-name ellipsis">{ws.name}</span>
              <span className="ws-menu-role">{t(`role.${ws.role}`)}</span>
            </span>
            <Check className="menu-check" aria-hidden />
          </div>
          {can.viewMembers(ws) ? (
            <div className="ws-menu-actions">
              <Link to={`/${ws.slug}/settings`} className="btn btn-sm" role="menuitem" onClick={close}>
                <Settings />
                {t("nav.settings")}
              </Link>
              <Link to={`/${ws.slug}/settings/members`} className="btn btn-sm" role="menuitem" onClick={close}>
                {can.invite(ws) ? <UserPlus /> : <UserRound />}
                {can.invite(ws) ? t("nav.invite") : t("nav.members")}
              </Link>
            </div>
          ) : null}
        </div>
        {others.map((w) => (
          <Link key={w.id} to={`/${w.slug}`} className="menu-item ws-menu-row" role="menuitem" onClick={close}>
            <WsMark name={w.name} logo={w.logo} size={32} />
            <span className="grow">
              <span className="ws-menu-name ellipsis">{w.name}</span>
              <span className="ws-menu-role">{t(`role.${w.role}`)}</span>
            </span>
          </Link>
        ))}
        <div className="menu-sep" />
        <Link to="/create-workspace" className="menu-item" role="menuitem" onClick={close}>
          <CirclePlus />
          {t("nav.createWorkspace")}
        </Link>
      </div>
    </Popover>
  );
}

function HelpMenu() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const version = useQuery({ queryKey: ["version"], queryFn: () => get<{ version: string }>("/version"), staleTime: Infinity });
  return (
    <Popover
      open={open}
      onClose={close}
      right
      trigger={
        <button className="icon-btn topbar-btn" onClick={() => setOpen(!open)} aria-label={t("nav.help")} title={t("nav.help")} aria-haspopup="menu" aria-expanded={open}>
          <CircleHelp />
        </button>
      }
    >
      <div className="menu" role="menu">
        <button className="menu-item" role="menuitem" onClick={() => (close(), emitShell("shortcuts"))}>
          <Keyboard />
          <span className="grow">{t("pal.shortcuts")}</span>
          <kbd className="keycap">?</kbd>
        </button>
        <a className="menu-item" role="menuitem" href="https://github.com/azamrizki/nextplane#readme" target="_blank" rel="noreferrer" onClick={close}>
          <FileText />
          {t("nav.docs")}
        </a>
        <div className="menu-sep" />
        <div className="menu-label">NextPlane {version.data ? `v${version.data.version}` : ""}</div>
      </div>
    </Popover>
  );
}

function UserMenu() {
  const t = useT();
  const me = useMe().data!;
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
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
      onClose={close}
      right
      trigger={
        <button className="icon-btn topbar-btn user-btn" onClick={() => setOpen((o) => !o)} aria-haspopup="menu" aria-expanded={open} aria-label={t("nav.account")} title={me.name || me.email}>
          <Avatar name={me.name} email={me.email} src={me.avatar} size={22} />
        </button>
      }
    >
      <div className="menu user-menu-panel" role="menu">
        <div className="user-card">
          <Avatar name={me.name} email={me.email} src={me.avatar} size={40} />
          <span className="user-card-name ellipsis">{me.name || me.email}</span>
          <span className="user-card-email ellipsis">{me.email}</span>
        </div>
        <Link to="/profile" className="menu-item" role="menuitem" onClick={close}>
          <Settings />
          {t("nav.profile")}
        </Link>
        {me.is_instance_admin ? (
          <Link to="/admin" className="menu-item" role="menuitem" onClick={close}>
            <ShieldCheck />
            {t("nav.admin")}
          </Link>
        ) : null}
        <div className="menu-sep" />
        <button className="menu-item danger" role="menuitem" onClick={() => logout.mutate()}>
          <LogOut />
          {t("shell.logout")}
        </button>
      </div>
    </Popover>
  );
}

/** A trigger plus a floating panel that closes on outside click and Esc. */
export function Popover({ trigger, children, open, onClose, up, right }: { trigger: ReactNode; children: ReactNode; open: boolean; onClose: () => void; up?: boolean; right?: boolean }) {
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
      {open ? <div className={"popover" + (up ? " popover-up" : "") + (right ? " popover-right" : "")}>{children}</div> : null}
    </div>
  );
}

/** Simple centered page frame for pages outside a workspace (profile, admin, create). */
export function PlainPage({ children, back }: { children: ReactNode; back?: boolean }) {
  const t = useT();
  return (
    <div className="plain-page">
      {back ? (
        <Link to="/" className="small back-link">
          <ChevronRight className="flip" aria-hidden /> {t("common.back")}
        </Link>
      ) : null}
      {children}
    </div>
  );
}

