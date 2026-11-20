import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { del, get, getAll, patch, post } from "../api/client";
import type { Activity, Relation, WorkItem } from "../api/types";
import { projectPath } from "./projects";

export const wiq = {
  list: (slug: string, pid: string, params = "") => ["work-items", slug, pid, params] as const,
  allLists: (slug: string, pid: string) => ["work-items", slug, pid] as const,
  one: (slug: string, pid: string, id: string) => ["work-item", slug, pid, id] as const,
  activity: (slug: string, pid: string, id: string) => ["work-item", slug, pid, id, "activity"] as const,
  relations: (slug: string, pid: string, id: string) => ["work-item", slug, pid, id, "relations"] as const,
};

export const itemsPath = (slug: string, pid: string, id?: string) => `${projectPath(slug, pid)}/work-items${id ? `/${id}` : ""}`;

export function useWorkItems(slug: string, pid: string, params = "", enabled = true) {
  return useQuery({
    queryKey: wiq.list(slug, pid, params),
    queryFn: () => getAll<WorkItem>(itemsPath(slug, pid) + (params ? `?${params}` : "")),
    enabled,
  });
}

export function useWorkItem(slug: string, pid: string, id: string) {
  return useQuery({ queryKey: wiq.one(slug, pid, id), queryFn: () => get<WorkItem>(itemsPath(slug, pid, id)) });
}

export function useActivity(slug: string, pid: string, id: string) {
  return useQuery({ queryKey: wiq.activity(slug, pid, id), queryFn: () => get<Activity[]>(itemsPath(slug, pid, id) + "/activity") });
}

export function useRelations(slug: string, pid: string, id: string) {
  return useQuery({ queryKey: wiq.relations(slug, pid, id), queryFn: () => get<Relation[]>(itemsPath(slug, pid, id) + "/relations") });
}

/** Patch one item, optimistically in its detail cache and every list of the project. */
export function useUpdateItem(slug: string, pid: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { id: string; body: Partial<WorkItem> & Record<string, unknown> }) => patch<WorkItem>(itemsPath(slug, pid, v.id), v.body),
    onMutate: async ({ id, body }) => {
      await qc.cancelQueries({ queryKey: wiq.allLists(slug, pid) });
      const lists = qc.getQueriesData<WorkItem[]>({ queryKey: wiq.allLists(slug, pid) });
      for (const [key, data] of lists) {
        if (data) qc.setQueryData(key, data.map((w) => (w.id === id ? { ...w, ...body } : w)));
      }
      const one = qc.getQueryData<WorkItem>(wiq.one(slug, pid, id));
      if (one) qc.setQueryData(wiq.one(slug, pid, id), { ...one, ...body });
      return { lists, one };
    },
    onError: (e, { id }, ctx) => {
      for (const [key, data] of ctx?.lists ?? []) qc.setQueryData(key, data);
      if (ctx?.one) qc.setQueryData(wiq.one(slug, pid, id), ctx.one);
      // On a conflict, load what the other person saved.
      if ((e as { status?: number }).status === 409) qc.invalidateQueries({ queryKey: wiq.one(slug, pid, id) });
    },
    onSuccess: (item) => {
      const one = qc.getQueryData<WorkItem>(wiq.one(slug, pid, item.id));
      qc.setQueryData(wiq.one(slug, pid, item.id), { ...one, ...item });
      qc.invalidateQueries({ queryKey: wiq.allLists(slug, pid) });
      qc.invalidateQueries({ queryKey: wiq.activity(slug, pid, item.id) });
      invalidateDerived(qc, slug, pid);
    },
  });
}

export function useCreateItem(slug: string, pid: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: Record<string, unknown>) => post<WorkItem>(itemsPath(slug, pid), body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: wiq.allLists(slug, pid) });
      invalidateDerived(qc, slug, pid);
    },
  });
}

export function useDeleteItem(slug: string, pid: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => del<void>(itemsPath(slug, pid, id)),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: wiq.allLists(slug, pid) });
      invalidateDerived(qc, slug, pid);
    },
  });
}

export function useArchiveItem(slug: string, pid: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { id: string; on: boolean }) => post<WorkItem>(itemsPath(slug, pid, v.id) + (v.on ? "/archive" : "/unarchive")),
    onSuccess: (item) => {
      qc.setQueryData(wiq.one(slug, pid, item.id), item);
      qc.invalidateQueries({ queryKey: wiq.allLists(slug, pid) });
      qc.invalidateQueries({ queryKey: wiq.activity(slug, pid, item.id) });
      invalidateDerived(qc, slug, pid);
    },
  });
}

/** Things derived from work items (cycle progress and counts) that must refresh too. */
function invalidateDerived(qc: ReturnType<typeof useQueryClient>, slug: string, pid: string) {
  qc.invalidateQueries({ queryKey: ["cycles", slug, pid] });
  qc.invalidateQueries({ queryKey: ["cycle", slug, pid] });
  qc.invalidateQueries({ queryKey: ["modules", slug, pid] });
  qc.invalidateQueries({ queryKey: ["module", slug, pid] });
  qc.invalidateQueries({ queryKey: ["my-work", slug] });
}

export const keyOf = (identifier: string, seq: number) => `${identifier}-${seq}`;
