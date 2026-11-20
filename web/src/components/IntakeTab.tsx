import { NavLink } from "react-router";
import { useI18n } from "../i18n";
import { useIntakeCount } from "../lib/intake";

/** The project's Intake tab with the number of open requests. */
export function IntakeTab({ slug, pid, to }: { slug: string; pid: string; to: string }) {
  const { t } = useI18n();
  const count = useIntakeCount(slug, pid).data?.open ?? 0;
  return (
    <NavLink to={to} className={({ isActive }) => (isActive ? "on" : "")}>
      {t("ptab.intake")}
      {count > 0 ? <span className="tab-count" aria-label={`(${count})`}>{count}</span> : null}
    </NavLink>
  );
}
