import { useQuery } from "@tanstack/react-query";
import { get } from "../api/client";
import type { Priority, StateGroup } from "../api/types";

export type WidgetKey = "quick_links" | "recents" | "stickies";
export interface Widget {
  key: WidgetKey;
  is_enabled: boolean;
  sort_order: number;
}
export interface QuickLink {
  id: string;
  url: string;
  title: string;
  created_at: string;
  updated_at: string;
}
export type RecentEntity = "work_item" | "page" | "project";
export type Recent =
  | {
      entity: "work_item";
      visited_at: string;
      id: string;
      project_id: string;
      identifier: string;
      sequence: number;
      title: string;
      priority: Priority;
      assignee_ids: string[];
      state: { name: string; color: string; group: StateGroup } | null;
      archived: boolean;
    }
  | { entity: "page"; visited_at: string; id: string; project_id: string; title: string; owner_id: string; project_name: string }
  | { entity: "project"; visited_at: string; id: string; name: string; identifier: string; icon: string | null; cover_color: string | null };

export const STICKY_COLORS = ["gray", "peach", "pink", "orange", "green", "light-blue", "dark-blue", "purple"] as const;
export type StickyColor = (typeof STICKY_COLORS)[number];
export interface Sticky {
  id: string;
  description_json: unknown;
  description_html: string;
  color: StickyColor;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export const hq = {
  widgets: (slug: string) => ["home", slug, "widgets"] as const,
  links: (slug: string) => ["home", slug, "links"] as const,
  recents: (slug: string, entity = "") => ["home", slug, "recents", entity] as const,
  allRecents: (slug: string) => ["home", slug, "recents"] as const,
  stickies: (slug: string, q = "", limit = 30) => ["home", slug, "stickies", q, limit] as const,
  allStickies: (slug: string) => ["home", slug, "stickies"] as const,
};

export const useWidgets = (slug: string) => useQuery({ queryKey: hq.widgets(slug), queryFn: () => get<Widget[]>(`/workspaces/${slug}/home/widgets`) });
export const useQuickLinks = (slug: string) => useQuery({ queryKey: hq.links(slug), queryFn: () => get<QuickLink[]>(`/workspaces/${slug}/quick-links`) });
export const useRecents = (slug: string, entity: RecentEntity | "" = "") =>
  useQuery({ queryKey: hq.recents(slug, entity), queryFn: () => get<Recent[]>(`/workspaces/${slug}/recents${entity ? `?entity=${entity}` : ""}`) });
export const useStickies = (slug: string, q = "", limit = 30) =>
  useQuery({ queryKey: hq.stickies(slug, q, limit), queryFn: () => get<Sticky[]>(`/workspaces/${slug}/stickies?limit=${limit}${q.trim() ? `&q=${encodeURIComponent(q.trim())}` : ""}`) });

const seen = new Map<string, number>();
/** Notes that you opened something, for Home's recents; at most once a minute per thing. */
export function recordVisit(slug: string, entity: RecentEntity, id: string, projectId?: string) {
  const key = `${slug}:${entity}:${id}`;
  const now = Date.now();
  if ((seen.get(key) ?? 0) > now - 60_000) return;
  seen.set(key, now);
  // keepalive: the visit still arrives when the person moves on at once.
  void fetch(`/api/v1/workspaces/${slug}/recents`, {
    method: "POST",
    credentials: "same-origin",
    keepalive: true,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ entity, id, project_id: projectId }),
  }).then((r) => (r.ok ? null : seen.delete(key)), () => seen.delete(key));
}

/** The host of a link, for its card ("docs.example.com"). */
export function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}
