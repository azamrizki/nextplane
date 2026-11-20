import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ApiError, get } from "../api/client";
import type { InstanceInfo, User } from "../api/types";

export const qk = {
  instance: ["instance"] as const,
  me: ["me"] as const,
};

export function useInstance() {
  return useQuery({ queryKey: qk.instance, queryFn: () => get<InstanceInfo>("/instance") });
}

/** The logged-in user, or `null` when there is no session (401 is not an error here). */
export function useMe() {
  return useQuery({
    queryKey: qk.me,
    queryFn: async () => {
      try {
        return await get<User>("/me");
      } catch (e) {
        if (e instanceof ApiError && e.status === 401) return null;
        throw e;
      }
    },
  });
}

export function useSetMe() {
  const qc = useQueryClient();
  return (u: User | null) => qc.setQueryData(qk.me, u);
}
