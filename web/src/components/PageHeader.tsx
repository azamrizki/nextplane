import { Archive, Boxes, ChartColumn, ChevronRight, Contrast, FileText, Import, ListTodo, PanelLeft, Settings } from "lucide-react";
import { Fragment, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router";
import type { Project } from "../api/types";
import { useI18n } from "../i18n";
import type { MessageKey } from "../i18n/en";
import { useProjects } from "../lib/projects";
import { useShell } from "../lib/shell";
import { ProjectIcon } from "../pages/Projects";
import { Picker } from "./Picker";

export interface Crumb {
  label?: ReactNode;
  to?: string;
  icon?: ReactNode;
  /** A ready-made crumb (e.g. the project switcher) instead of label/to. */
  node?: ReactNode;
}

/** The 48px bar on top of every page: breadcrumbs on the left (the last one is the page
 * title), the page's actions on the right. */
export function PageHeader({ crumbs, actions, extra }: { crumbs: Crumb[]; actions?: ReactNode; extra?: ReactNode }) {
  const { t } = useI18n();
  const shell = useShell();
  return (
    <header className="page-header">
      {shell.collapsed ? (
        <button className="icon-btn" onClick={shell.toggle} aria-label={t("nav.expand")} title={`${t("nav.expand")} (Ctrl/⌘ B)`}>
          <PanelLeft />
        </button>
      ) : null}
      <nav className="crumbs" aria-label={t("nav.breadcrumbs")}>
        {crumbs.map((c, i) => {
          const last = i === crumbs.length - 1;
          return (
            <Fragment key={i}>
              {c.node ? (
                c.node
              ) : c.to && !last ? (
                <Link to={c.to} className="crumb crumb-link">
                  {c.icon ?? null}
                  <span className="crumb-label">{c.label}</span>
                </Link>
              ) : (
                <span className={"crumb" + (last ? " crumb-current" : "")} aria-current={last ? "page" : undefined}>
                  {c.icon ?? null}
                  {last ? <h1 className="crumb-label">{c.label}</h1> : <span className="crumb-label">{c.label}</span>}
                </span>
              )}
              {!last && !c.node ? <ChevronRight className="crumb-sep" aria-hidden /> : null}
            </Fragment>
          );
        })}
        {extra ?? null}
      </nav>
      <div className="page-header-actions">{actions ?? null}</div>
    </header>
  );
}

/** The project crumb: the name goes to the project's work items; the chevron next to it
 * (which also separates it from the next crumb) opens a project switcher. */
export function ProjectCrumb({ slug, project, section }: { slug: string; project: Project; section?: string }) {
  const { t } = useI18n();
  const projects = useProjects(slug);
  const navigate = useNavigate();
  const [, setOpen] = useState(false);
  const mine = (projects.data ?? []).filter((p) => p.is_member && !p.archived_at);
  return (
    <span className="crumb-project">
      <Link to={`/${slug}/projects/${project.id}`} className="crumb crumb-link">
        <ProjectIcon p={project} size={16} />
        <span className="crumb-label">{project.name}</span>
      </Link>
      <Picker
        label={t("nav.switchProject")}
        className="crumb-switch"
        value={[project.id]}
        onChange={(v) => {
          setOpen(false);
          if (v[0] && v[0] !== project.id) navigate(`/${slug}/projects/${v[0]}${section ? `/${section}` : ""}`);
        }}
        options={mine.map((p) => ({ value: p.id, label: p.name, icon: <ProjectIcon p={p} size={16} /> }))}
        trigger={<ChevronRight className="crumb-chevron" aria-hidden />}
      />
    </span>
  );
}

const SECTIONS: Record<string, { icon: ReactNode; key: MessageKey }> = {
  "": { icon: <ListTodo />, key: "ptab.items" },
  "work-items": { icon: <ListTodo />, key: "ptab.items" },
  cycles: { icon: <Contrast />, key: "ptab.cycles" },
  modules: { icon: <Boxes />, key: "ptab.modules" },
  pages: { icon: <FileText />, key: "ptab.pages" },
  intake: { icon: <Import />, key: "ptab.intake" },
  analytics: { icon: <ChartColumn />, key: "ptab.analytics" },
  settings: { icon: <Settings />, key: "nav.settings" },
  archives: { icon: <Archive />, key: "ptab.archives" },
};

/** A project page's header: project crumb (with a project switcher), the section, any
 * deeper crumbs, then the page's actions on the right. */
export function ProjectBar({ ctx, children, crumbs, extra }: { ctx: { project: Project }; children?: ReactNode; crumbs?: Crumb[]; extra?: ReactNode }) {
  const { t } = useI18n();
  const { slug = "" } = useParams();
  const { pathname } = useLocation();
  const p = ctx.project;
  const base = `/${slug}/projects/${p.id}`;
  const rest = pathname.startsWith(base) ? (pathname.slice(base.length).split("/")[1] ?? "") : "";
  const sec = SECTIONS[rest] ?? SECTIONS[""]!;
  const section = rest === "work-items" ? "" : rest;
  const list: Crumb[] = [
    { node: <ProjectCrumb slug={slug} project={p} section={section} /> },
    { icon: sec.icon, label: t(sec.key), to: section ? `${base}/${section}` : base },
    ...(crumbs ?? []),
  ];
  return <PageHeader crumbs={list} actions={children} extra={extra} />;
}

