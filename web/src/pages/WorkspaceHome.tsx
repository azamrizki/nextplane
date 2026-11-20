import { Link, useOutletContext } from "react-router";
import type { MyWorkspace } from "../api/types";
import { useT } from "../i18n";

export function useWs() {
  return useOutletContext<MyWorkspace>();
}

export function WorkspaceHomePage() {
  const t = useT();
  const ws = useWs();
  return (
    <div className="page">
      <div className="page-head">
        <h1>{ws.name}</h1>
        <span className="badge">{t(`role.${ws.role}`)}</span>
      </div>
      <Link to={`/${ws.slug}/projects`} className="card empty">{t("nav.projects")} →</Link>
    </div>
  );
}
