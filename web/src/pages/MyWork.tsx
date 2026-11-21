import { Navigate } from "react-router";
import { useMe } from "../lib/session";
import { useWs } from "./WorkspaceHome";

/** "Your work" is your own profile (as in Plane); the old address opens it. */
export function MyWorkRedirect() {
  const ws = useWs();
  const me = useMe().data!;
  return <Navigate to={`/${ws.slug}/profile/${me.id}/assigned`} replace />;
}
