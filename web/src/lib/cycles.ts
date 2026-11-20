import { useQuery } from "@tanstack/react-query";
import { get } from "../api/client";
import type { Cycle, CycleProgress } from "../api/types";
import { projectPath } from "./projects";

export const cq = {
  list: (slug: string, pid: string, archived = false) => ["cycles", slug, pid, archived] as const,
  all: (slug: string, pid: string) => ["cycles", slug, pid] as const,
  one: (slug: string, pid: string, id: string) => ["cycle", slug, pid, id] as const,
  progress: (slug: string, pid: string, id: string) => ["cycle", slug, pid, id, "progress"] as const,
};

export const cyclesPath = (slug: string, pid: string, id?: string) => `${projectPath(slug, pid)}/cycles${id ? `/${id}` : ""}`;

export const useCycles = (slug: string, pid: string, enabled = true, archived = false) =>
  useQuery({ queryKey: cq.list(slug, pid, archived), queryFn: () => get<Cycle[]>(cyclesPath(slug, pid) + (archived ? "?archived=true" : "")), enabled });
export const useCycle = (slug: string, pid: string, id: string) =>
  useQuery({ queryKey: cq.one(slug, pid, id), queryFn: () => get<Cycle>(cyclesPath(slug, pid, id)) });
export const useCycleProgress = (slug: string, pid: string, id: string) =>
  useQuery({ queryKey: cq.progress(slug, pid, id), queryFn: () => get<CycleProgress>(cyclesPath(slug, pid, id) + "/progress") });
