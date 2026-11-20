import { Navigate } from "react-router";
import { FullPageSpinner } from "../components/ui";
import { lastWorkspace, useWorkspaces } from "../lib/queries";

/** `/`: the last workspace used, else the first, else the create page. */
export function RootRedirect() {
  const list = useWorkspaces();
  if (list.isLoading) return <FullPageSpinner />;
  const all = list.data ?? [];
  if (all.length === 0) return <Navigate to="/create-workspace" replace />;
  const last = lastWorkspace();
  const ws = all.find((w) => w.slug === last) ?? all[0]!;
  return <Navigate to={`/${ws.slug}`} replace />;
}
