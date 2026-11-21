import { useQueries } from "@tanstack/react-query";
import { useMemo } from "react";
import { get } from "../api/client";
import type { Cycle, Label, Module, Project, ProjMember, WfState } from "../api/types";
import type { PropsCtx } from "../components/WorkItemProps";
import { pcan } from "./perm";
import { pq, projectPath, useLabels, useProjectMembers, useStates } from "./projects";
import { cq, useCycles } from "./cycles";
import { mq, useModules } from "./modules";

/** Everything property controls need for one project: states, labels, members, editability. */
export function useProjectCtx(slug: string, project: Project | undefined): PropsCtx | null {
  const on = !!project?.my_role;
  const pid = project?.id ?? "";
  const states = useStates(slug, pid, on);
  const labels = useLabels(slug, pid, on);
  const members = useProjectMembers(slug, pid, on);
  const cycles = useCycles(slug, pid, on && !!project?.feature_cycles);
  const modules = useModules(slug, pid, on && !!project?.feature_modules);
  return useMemo(() => {
    if (!project || !states.data || !labels.data || !members.data) return null;
    return {
      project,
      states: states.data,
      labels: labels.data,
      members: members.data,
      cycles: project.feature_cycles ? (cycles.data ?? []) : [],
      modules: project.feature_modules ? (modules.data ?? []) : [],
      editable: pcan.editWorkItem(project) && !project.archived_at,
    };
  }, [project, states.data, labels.data, members.data, cycles.data, modules.data]);
}

/** Property contexts for several projects at once (workspace views), by project id. Projects
 * whose data has not arrived yet are missing from the map. */
export function useProjectCtxs(slug: string, projects: Project[]): Map<string, PropsCtx> {
  const q = (kind: "states" | "labels" | "members" | "cycles" | "modules", p: Project) => ({
    queryKey: kind === "cycles" ? cq.list(slug, p.id) : kind === "modules" ? mq.list(slug, p.id) : [...pq.one(slug, p.id), kind],
    queryFn: () => get<unknown[]>(`${projectPath(slug, p.id)}/${kind}`),
    enabled: !!p.my_role && (kind !== "cycles" || p.feature_cycles) && (kind !== "modules" || p.feature_modules),
  });
  const results = useQueries({
    queries: projects.flatMap((p) => [q("states", p), q("labels", p), q("members", p), q("cycles", p), q("modules", p)]),
  });
  const stamp = results.map((r) => r.dataUpdatedAt).join(",");
  return useMemo(() => {
    const out = new Map<string, PropsCtx>();
    projects.forEach((project, i) => {
      const [states, labels, members, cycles, modules] = results.slice(i * 5, i * 5 + 5).map((r) => r.data);
      if (!states || !labels || !members) return;
      out.set(project.id, {
        project,
        states: states as WfState[],
        labels: labels as Label[],
        members: members as ProjMember[],
        cycles: project.feature_cycles ? ((cycles ?? []) as Cycle[]) : [],
        modules: project.feature_modules ? ((modules ?? []) as Module[]) : [],
        editable: pcan.editWorkItem(project) && !project.archived_at,
      });
    });
    return out;
    // `stamp` changes whenever any of the queries has new data.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projects, stamp]);
}
