import { useQuery } from "@tanstack/react-query";
import { get } from "../api/client";
import type { Label, Project, ProjMember, WfState } from "../api/types";

export const pq = {
  list: (slug: string, archived = false) => ["projects", slug, archived] as const,
  all: (slug: string) => ["projects", slug] as const,
  one: (slug: string, id: string) => ["project", slug, id] as const,
  members: (slug: string, id: string) => ["project", slug, id, "members"] as const,
  states: (slug: string, id: string) => ["project", slug, id, "states"] as const,
  labels: (slug: string, id: string) => ["project", slug, id, "labels"] as const,
};

const base = (slug: string, id?: string) => `/workspaces/${slug}/projects${id ? `/${id}` : ""}`;
export const projectPath = base;

export const useProjects = (slug: string, archived = false) =>
  useQuery({ queryKey: pq.list(slug, archived), queryFn: () => get<Project[]>(base(slug) + (archived ? "?archived=true" : "")) });
export const useProject = (slug: string, id: string) =>
  useQuery({ queryKey: pq.one(slug, id), queryFn: () => get<Project>(base(slug, id)) });
export const useProjectMembers = (slug: string, id: string, enabled = true) =>
  useQuery({ queryKey: pq.members(slug, id), queryFn: () => get<ProjMember[]>(base(slug, id) + "/members"), enabled });
export const useStates = (slug: string, id: string, enabled = true) =>
  useQuery({ queryKey: pq.states(slug, id), queryFn: () => get<WfState[]>(base(slug, id) + "/states"), enabled });
export const useLabels = (slug: string, id: string, enabled = true) =>
  useQuery({ queryKey: pq.labels(slug, id), queryFn: () => get<Label[]>(base(slug, id) + "/labels"), enabled });

export const PALETTE = ["#8b8d98", "#e5484d", "#f76b15", "#e2a336", "#30a46c", "#12a594", "#0e7c6b", "#3e63dd", "#6e56cf", "#d6409f", "#ad7f58"];
