import { useQuery } from "@tanstack/react-query";
import { get } from "../api/client";
import type { Page, PageVersion } from "../api/types";
import { projectPath } from "./projects";

export const pgq = {
  list: (slug: string, pid: string, archived = false) => ["pages", slug, pid, archived] as const,
  all: (slug: string, pid: string) => ["pages", slug, pid] as const,
  one: (slug: string, pid: string, id: string) => ["page", slug, pid, id] as const,
  versions: (slug: string, pid: string, id: string) => ["page", slug, pid, id, "versions"] as const,
};
export const pagesPath = (slug: string, pid: string, id?: string) => `${projectPath(slug, pid)}/pages${id ? `/${id}` : ""}`;

export const usePages = (slug: string, pid: string, archived = false, enabled = true) =>
  useQuery({ queryKey: pgq.list(slug, pid, archived), queryFn: () => get<Page[]>(pagesPath(slug, pid) + (archived ? "?archived=true" : "")), enabled });
export const usePage = (slug: string, pid: string, id: string) =>
  useQuery({ queryKey: pgq.one(slug, pid, id), queryFn: () => get<Page>(pagesPath(slug, pid, id)), enabled: !!id });
export const usePageVersions = (slug: string, pid: string, id: string, enabled: boolean) =>
  useQuery({ queryKey: pgq.versions(slug, pid, id), queryFn: () => get<PageVersion[]>(pagesPath(slug, pid, id) + "/versions"), enabled });

/** Pages as a tree in display order: [page, depth][]. */
export function flattenTree(pages: Page[]): [Page, number][] {
  const kids = new Map<string | null, Page[]>();
  const ids = new Set(pages.map((p) => p.id));
  for (const p of pages) {
    const parent = p.parent_id && ids.has(p.parent_id) ? p.parent_id : null;
    kids.set(parent, [...(kids.get(parent) ?? []), p]);
  }
  const out: [Page, number][] = [];
  const walk = (parent: string | null, depth: number) => {
    for (const p of (kids.get(parent) ?? []).sort((a, b) => a.sort_order - b.sort_order)) {
      out.push([p, depth]);
      walk(p.id, depth + 1);
    }
  };
  walk(null, 0);
  return out;
}

export type DropZone = "before" | "inside" | "after";

/** The parent as the tree shows it (a parent the viewer cannot see counts as top level). */
function shownParent(pages: Page[], p: Page): string | null {
  return p.parent_id && pages.some((q) => q.id === p.parent_id) ? p.parent_id : null;
}

/** Whether `id` is `ancestor` or sits somewhere below it. */
export function isWithin(pages: Page[], id: string, ancestor: string): boolean {
  const byId = new Map(pages.map((p) => [p.id, p]));
  let cur: string | null = id;
  for (let n = 0; cur && n < 64; n++) {
    if (cur === ancestor) return true;
    cur = byId.get(cur)?.parent_id ?? null;
  }
  return false;
}

/** Where a dragged page lands when dropped before, inside or after another one. */
export function planMove(pages: Page[], dragId: string, targetId: string, zone: DropZone): { parent_id: string | null; sort_order: number } | null {
  const target = pages.find((p) => p.id === targetId);
  if (!target || isWithin(pages, targetId, dragId)) return null;
  if (zone === "inside") {
    const kids = pages.filter((p) => p.parent_id === targetId && p.id !== dragId);
    return { parent_id: targetId, sort_order: kids.length ? Math.max(...kids.map((k) => k.sort_order)) + 1000 : 1000 };
  }
  const parent = shownParent(pages, target);
  const sibs = pages.filter((p) => p.id !== dragId && shownParent(pages, p) === parent).sort((a, b) => a.sort_order - b.sort_order);
  const i = sibs.findIndex((p) => p.id === targetId);
  const next = zone === "before" ? sibs[i - 1] : sibs[i + 1];
  const sort_order = next ? (target.sort_order + next.sort_order) / 2 : target.sort_order + (zone === "before" ? -1000 : 1000);
  return { parent_id: parent, sort_order };
}

/** Text selected in the editor as a work item title. */
export function selectionTitle(text: string): string {
  return text.replace(/\s+/g, " ").trim().slice(0, 255);
}
