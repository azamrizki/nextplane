import { Bot, Boxes, Building2, CircleDot, Database, GitBranch, Settings, Share2, Tag, Triangle, ToggleLeft, UsersRound, Webhook } from "lucide-react";
import type { ReactNode } from "react";
import { NavLink, useParams } from "react-router";
import type { MyWorkspace } from "../api/types";
import { useI18n } from "../i18n";
import { can } from "../lib/perm";
import { PageHeader, type Crumb } from "./PageHeader";

/** Plane's settings layout: header on top, a settings menu on the left, the form on the right. */
export function SettingsLayout({ nav, children }: { nav: ReactNode; children: ReactNode }) {
  return (
    <div className="settings-layout">
      <nav className="settings-nav">{nav}</nav>
      <div className="page settings-content">{children}</div>
    </div>
  );
}

/** Workspace settings pages: General, Members, Webhooks, GitHub. */
export function WsSettingsFrame({ ws, crumbs, actions, children }: { ws: MyWorkspace; crumbs?: Crumb[]; actions?: ReactNode; children: ReactNode }) {
  const { t } = useI18n();
  const base = `/${ws.slug}/settings`;
  return (
    <div className="proj-view">
      <PageHeader crumbs={[{ icon: <Settings />, label: t("ws.settings.title"), to: base }, ...(crumbs ?? [])]} actions={actions} />
      <SettingsLayout
        nav={
          <>
            <div className="settings-nav-title">{t("nav.workspaceSection")}</div>
            <NavLink to={base} end className="side-link">
              <Building2 />
              {t("ws.settings.general")}
            </NavLink>
            {can.viewMembers(ws) ? (
              <NavLink to={`${base}/members`} className="side-link">
                <UsersRound />
                {t("members.title")}
              </NavLink>
            ) : null}
            {can.editWorkspace(ws) ? (
              <>
                <NavLink to={`${base}/webhooks`} className="side-link">
                  <Webhook />
                  {t("wh.title")}
                </NavLink>
                <NavLink to={`${base}/github`} className="side-link">
                  <GitBranch />
                  {t("gh.title")}
                </NavLink>
              </>
            ) : null}
          </>
        }
      >
        {children}
      </SettingsLayout>
    </div>
  );
}

const PROJECT_TAB_ICON: Record<string, ReactNode> = {
  general: <Settings />,
  members: <UsersRound />,
  states: <CircleDot />,
  labels: <Tag />,
  estimates: <Triangle />,
  features: <ToggleLeft />,
  data: <Database />,
  publish: <Share2 />,
  bots: <Bot />,
  modules: <Boxes />,
};

/** The left menu of a project's settings. */
export function ProjectSettingsNav({ tabs }: { tabs: string[] }) {
  const { t } = useI18n();
  const { slug = "", projectId = "" } = useParams();
  const base = `/${slug}/projects/${projectId}/settings`;
  return (
    <>
      <div className="settings-nav-title">{t("proj.settings")}</div>
      {tabs.map((k) => (
        <NavLink key={k} to={k === "general" ? base : `${base}/${k}`} end className="side-link">
          {PROJECT_TAB_ICON[k] ?? <Settings />}
          {t(`proj.tab.${k}` as "proj.tab.general")}
        </NavLink>
      ))}
    </>
  );
}
