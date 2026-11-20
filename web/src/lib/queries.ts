import { useQuery } from "@tanstack/react-query";
import { get } from "../api/client";
import type { MyWorkspace } from "../api/types";

export const wq = {
  list: ["workspaces"] as const,
  one: (slug: string) => ["workspace", slug] as const,
  members: (slug: string) => ["workspace", slug, "members"] as const,
  invites: (slug: string) => ["workspace", slug, "invites"] as const,
  trash: ["workspaces", "trash"] as const,
};

export function useWorkspaces() {
  return useQuery({ queryKey: wq.list, queryFn: () => get<MyWorkspace[]>("/workspaces") });
}

export function useWorkspace(slug: string) {
  return useQuery({ queryKey: wq.one(slug), queryFn: () => get<MyWorkspace>(`/workspaces/${slug}`) });
}

const LAST_WS = "np.lastWorkspace";
export const rememberWorkspace = (slug: string) => localStorage.setItem(LAST_WS, slug);
export const lastWorkspace = () => localStorage.getItem(LAST_WS);
