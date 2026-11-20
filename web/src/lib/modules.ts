import { useQuery } from "@tanstack/react-query";
import { get } from "../api/client";
import type { CycleProgress, Module } from "../api/types";
import { projectPath } from "./projects";

export const mq = {
  list: (slug: string, pid: string, archived = false) => ["modules", slug, pid, archived] as const,
  all: (slug: string, pid: string) => ["modules", slug, pid] as const,
  one: (slug: string, pid: string, id: string) => ["module", slug, pid, id] as const,
  progress: (slug: string, pid: string, id: string) => ["module", slug, pid, id, "progress"] as const,
  links: (slug: string, pid: string, id: string) => ["module", slug, pid, id, "links"] as const,
};

export const modulesPath = (slug: string, pid: string, id?: string) => `${projectPath(slug, pid)}/modules${id ? `/${id}` : ""}`;

export const useModules = (slug: string, pid: string, enabled = true, archived = false) =>
  useQuery({ queryKey: mq.list(slug, pid, archived), queryFn: () => get<Module[]>(modulesPath(slug, pid) + (archived ? "?archived=true" : "")), enabled });
export const useModule = (slug: string, pid: string, id: string) =>
  useQuery({ queryKey: mq.one(slug, pid, id), queryFn: () => get<Module>(modulesPath(slug, pid, id)) });
export const useModuleProgress = (slug: string, pid: string, id: string) =>
  useQuery({ queryKey: mq.progress(slug, pid, id), queryFn: () => get<CycleProgress>(modulesPath(slug, pid, id) + "/progress") });
