import { useQuery } from "@tanstack/react-query";
import { Briefcase, CircleCheck, House, LayoutGrid, Settings, UserPlus, UserRound } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { Link, useOutletContext } from "react-router";
import { get } from "../api/client";
import type { MyWorkspace, WsMember } from "../api/types";
import { HomeWidgets, ManageWidgets } from "../components/HomeWidgets";
import { PageHeader } from "../components/PageHeader";
import { useI18n } from "../i18n";
import { useProjects } from "../lib/projects";
import { wq } from "../lib/queries";
import { useMe } from "../lib/session";

export function useWs() {
  return useOutletContext<MyWorkspace>();
}

/** Home, as in Plane: a greeting, a "get started" checklist for new workspaces, then the
 * person's widgets (quick links, recents, stickies) in their order. */
export function WorkspaceHomePage() {
  const { t } = useI18n();
  const ws = useWs();
  const [managing, setManaging] = useState(false);
  return (
    <div className="page home-page">
      <PageHeader
        crumbs={[{ icon: <House />, label: t("nav.home") }]}
        actions={
          <button className="btn" onClick={() => setManaging(true)}>
            <LayoutGrid />
            {t("hw.manage")}
          </button>
        }
      />
      <Greeting />
      <Quickstart ws={ws} />
      <HomeWidgets ws={ws} />
      <ManageWidgets ws={ws} open={managing} onClose={() => setManaging(false)} />
    </div>
  );
}

function Greeting() {
  const { t, lang } = useI18n();
  const me = useMe().data!;
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const h = setInterval(() => setNow(new Date()), 30_000);
    return () => clearInterval(h);
  }, []);
  const hour = now.getHours();
  const greet = hour < 12 ? "home.morning" : hour < 18 ? "home.afternoon" : "home.evening";
  const locale = lang === "id" ? "id-ID" : "en-US";
  return (
    <div className="home-hello">
      <h2>{t(greet, { name: me.name || me.email.split("@")[0]! })}</h2>
      <span className="muted">
        {now.toLocaleDateString(locale, { weekday: "long", month: "short", day: "numeric" })} · {now.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit", hour12: false })}
      </span>
    </div>
  );
}

/** Until it is dismissed, or the person has joined a project and the workspace has someone
 * else in it (as in Plane). */
function Quickstart({ ws }: { ws: MyWorkspace }) {
  const { t } = useI18n();
  const me = useMe().data!;
  const key = `np.quickstart.${ws.slug}`;
  const [hidden, setHidden] = useState(() => localStorage.getItem(key) === "1");
  const projects = useProjects(ws.slug);
  const members = useQuery({ queryKey: wq.members(ws.slug), queryFn: () => get<WsMember[]>(`/workspaces/${ws.slug}/members`), enabled: !hidden });
  if (hidden || !projects.isSuccess || !members.isSuccess) return null;
  const joined = projects.data.some((p) => p.is_member);
  const team = members.data.length >= 2;
  if (joined && team) return null;
  const admin = ws.role === "owner" || ws.role === "admin";
  const steps: { done: boolean; icon: ReactNode; label: string; to: string; show: boolean }[] = [
    { done: joined, icon: <Briefcase />, label: t("hw.qsProject"), to: `/${ws.slug}/projects?new=1`, show: ws.role !== "guest" },
    { done: team, icon: <UserPlus />, label: t("hw.qsInvite"), to: `/${ws.slug}/settings/members`, show: admin },
    { done: !!ws.logo, icon: <Settings />, label: t("hw.qsWorkspace"), to: `/${ws.slug}/settings`, show: admin },
    { done: !!me.avatar, icon: <UserRound />, label: t("hw.qsProfile"), to: "/profile", show: true },
  ];
  return (
    <section className="quickstart" aria-label={t("hw.quickstart")}>
      <div className="widget-head">
        <h3>{t("hw.quickstart")}</h3>
        <span className="grow" />
        <button
          className="btn btn-sm btn-ghost"
          onClick={() => {
            localStorage.setItem(key, "1");
            setHidden(true);
          }}
        >
          {t("hw.qsDismiss")}
        </button>
      </div>
      <div className="quickstart-steps">
        {steps
          .filter((s) => s.show)
          .map((s) => (
            <Link key={s.label} to={s.to} className={"quickstart-step" + (s.done ? " done" : "")}>
              <span className="quickstart-icon" aria-hidden>
                {s.done ? <CircleCheck /> : s.icon}
              </span>
              <span>{s.label}</span>
            </Link>
          ))}
      </div>
    </section>
  );
}
