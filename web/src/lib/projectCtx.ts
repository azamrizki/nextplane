import { useMemo } from "react";
import type { Project } from "../api/types";
import type { PropsCtx } from "../components/WorkItemProps";
import { pcan } from "./perm";
import { useLabels, useProjectMembers, useStates } from "./projects";
import { useCycles } from "./cycles";
import { useModules } from "./modules";

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
