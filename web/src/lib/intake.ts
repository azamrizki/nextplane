import { useQuery } from "@tanstack/react-query";
import { get } from "../api/client";
import type { IntakeEntry } from "../api/types";
import { projectPath } from "./projects";

export type IntakeViewName = "open" | "snoozed" | "closed";

export const iq = {
  all: (slug: string, pid: string) => ["intake", slug, pid] as const,
  list: (slug: string, pid: string, view: IntakeViewName) => ["intake", slug, pid, "list", view] as const,
  one: (slug: string, pid: string, id: string) => ["intake", slug, pid, "one", id] as const,
  count: (slug: string, pid: string) => ["intake", slug, pid, "count"] as const,
  form: (slug: string, pid: string) => ["intake", slug, pid, "form"] as const,
};
export const intakePath = (slug: string, pid: string, id?: string) => `${projectPath(slug, pid)}/intake${id ? `/${id}` : ""}`;

export const useIntake = (slug: string, pid: string, view: IntakeViewName, enabled = true) =>
  useQuery({ queryKey: iq.list(slug, pid, view), queryFn: () => get<IntakeEntry[]>(`${intakePath(slug, pid)}?view=${view}`), enabled });
export const useIntakeEntry = (slug: string, pid: string, id: string) =>
  useQuery({ queryKey: iq.one(slug, pid, id), queryFn: () => get<IntakeEntry>(intakePath(slug, pid, id)), enabled: !!id });
export const useIntakeCount = (slug: string, pid: string) =>
  useQuery({ queryKey: iq.count(slug, pid), queryFn: () => get<{ open: number }>(`${intakePath(slug, pid)}/count`) });

/** Whether `me` may still change a request (mirrors the server's rule). */
export function mayEditIntake(e: IntakeEntry, me: string, isAdmin: boolean, projectArchived: boolean): boolean {
  if (projectArchived || e.status === "accepted") return false;
  if (isAdmin) return true;
  return e.created_by === me && (e.status === "pending" || e.status === "snoozed");
}
