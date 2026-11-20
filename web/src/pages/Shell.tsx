import { Navigate, Outlet, useLocation } from "react-router";
import { FullPageSpinner } from "../components/ui";
import { useInstance, useMe } from "../lib/session";

/** Every logged-in page lives inside this: it sends anonymous visitors to login or setup. */
export function RequireAuth() {
  const instance = useInstance();
  const me = useMe();
  const location = useLocation();
  if (instance.isLoading || me.isLoading) return <FullPageSpinner />;
  if (instance.data?.setup_required) return <Navigate to="/setup" replace />;
  if (!me.data) return <Navigate to="/login" replace state={{ from: location.pathname + location.search }} />;
  return <Outlet />;
}

