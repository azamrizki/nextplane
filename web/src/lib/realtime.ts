// Live updates: one WebSocket per tab, subscribed to the current workspace. Each event
// invalidates the cached queries of the area that changed, so the screens refetch.
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { CLIENT_ID } from "../api/client";

interface Event {
  workspace: string;
  project_id: string | null;
  work_item_id: string | null;
  area: string;
  client: string | null;
}

export function applyEvent(qc: QueryClient, slug: string, e: Event) {
  if (e.area === "resync") {
    qc.invalidateQueries();
    return;
  }
  // Anyone's change in the workspace may have notified me.
  qc.invalidateQueries({ queryKey: ["notifications", slug] });
  if (e.client === CLIENT_ID) return;
  const pid = e.project_id;
  if (!pid) {
    qc.invalidateQueries({ queryKey: ["workspaces"] });
    qc.invalidateQueries({ queryKey: ["workspace", slug] });
    qc.invalidateQueries({ queryKey: ["projects", slug] });
    return;
  }
  if (e.area === "page") {
    // Page snapshots arrive every few seconds while someone types: refresh only pages.
    qc.invalidateQueries({ queryKey: ["pages", slug, pid] });
    qc.invalidateQueries({ queryKey: ["page", slug, pid] });
    return;
  }
  qc.invalidateQueries({ queryKey: ["projects", slug] });
  qc.invalidateQueries({ queryKey: ["project", slug, pid] });
  qc.invalidateQueries({ queryKey: ["work-items", slug, pid] });
  qc.invalidateQueries({ queryKey: ["cycles", slug, pid] });
  qc.invalidateQueries({ queryKey: ["cycle", slug, pid] });
  qc.invalidateQueries({ queryKey: ["modules", slug, pid] });
  qc.invalidateQueries({ queryKey: ["module", slug, pid] });
  qc.invalidateQueries({ queryKey: ["views", pid] });
  qc.invalidateQueries({ queryKey: ["my-work", slug] });
  if (e.work_item_id) {
    const id = e.work_item_id;
    qc.invalidateQueries({ queryKey: ["work-item", slug, pid, id] });
    for (const k of ["comments", "attachments", "links", "reactions"]) qc.invalidateQueries({ queryKey: [k, id] });
  }
}

/** Keeps a WebSocket open for `slug`, reconnecting with backoff. */
export function useRealtime(slug: string) {
  const qc = useQueryClient();
  useEffect(() => {
    if (!slug || typeof WebSocket === "undefined") return;
    let ws: WebSocket | null = null;
    let closed = false;
    let retry = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const connect = () => {
      const proto = location.protocol === "https:" ? "wss" : "ws";
      ws = new WebSocket(`${proto}://${location.host}/api/v1/ws`);
      ws.onopen = () => {
        if (retry > 0) qc.invalidateQueries(); // we may have missed events while away
        retry = 0;
        ws?.send(JSON.stringify({ subscribe: slug }));
      };
      ws.onmessage = (m) => {
        try {
          const e = JSON.parse(String(m.data));
          if ("area" in e) applyEvent(qc, slug, e as Event);
        } catch {
          /* ignore */
        }
      };
      ws.onclose = () => {
        if (closed) return;
        retry += 1;
        timer = setTimeout(connect, Math.min(30_000, 500 * 2 ** retry));
      };
    };
    connect();
    return () => {
      closed = true;
      clearTimeout(timer);
      ws?.close();
    };
  }, [slug, qc]);
}
