import { describe, expect, it } from "vitest";
import type { WfState, WorkItem } from "../api/types";
import { addDays, daysBetween, monthGrid, EMPTY_FILTERS, filtersToQuery, groupItems, patchForGroup, sortBetween, sortItems } from "./view";

const st = (id: string, group: WfState["group"], order: number): WfState => ({ id, name: id, color: "#000", group, sort_order: order, is_default: false });
const wi = (id: string, p: Partial<WorkItem> = {}): WorkItem => ({
  id, project_id: "p", sequence: 1, title: id, state_id: "todo", priority: "none", start_date: null, due_date: null, estimate: null,
  parent_id: null, sort_order: 0, created_by: "u", created_at: "2026-01-01", updated_at: "2026-01-01", completed_at: null, archived_at: null,
  assignee_ids: [], label_ids: [], sub_item_count: 0, ...p,
});

describe("view", () => {
  const ctx = {
    states: [st("done", "completed", 1), st("todo", "unstarted", 1), st("doing", "started", 1)],
    labels: [{ id: "bug", parent_id: null, name: "Bug", color: "#f00", sort_order: 1 }, { id: "ui", parent_id: null, name: "UI", color: "#00f", sort_order: 2 }],
    members: [{ user_id: "ana", email: "a@x", name: "Ana", avatar: null, role: "admin" as const, workspace_role: "owner" as const }],
  };

  it("groups by state in workflow order and keeps empty groups when asked", () => {
    const g = groupItems([wi("a", { state_id: "doing" }), wi("b")], "state", ctx, "manual", true);
    expect(g.map((x) => x.key)).toEqual(["todo", "doing", "done"]);
    expect(groupItems([wi("a")], "state", ctx, "manual", false).map((x) => x.key)).toEqual(["todo"]);
  });

  it("puts multi-label items in every label group plus a none group", () => {
    const g = groupItems([wi("a", { label_ids: ["bug", "ui"] }), wi("b")], "label", ctx, "manual", true);
    expect(g.find((x) => x.key === "bug")!.items.map((i) => i.id)).toEqual(["a"]);
    expect(g.find((x) => x.key === "ui")!.items.map((i) => i.id)).toEqual(["a"]);
    expect(g.find((x) => x.key === "none")!.items.map((i) => i.id)).toEqual(["b"]);
  });

  it("sorts by priority and by due date with empty dates last", () => {
    expect(sortItems([wi("a", { priority: "low" }), wi("b", { priority: "urgent" })], "priority").map((i) => i.id)).toEqual(["b", "a"]);
    expect(sortItems([wi("a"), wi("b", { due_date: "2026-02-01" }), wi("c", { due_date: "2026-01-01" })], "due_date").map((i) => i.id)).toEqual(["c", "b", "a"]);
  });

  it("builds the server query", () => {
    expect(filtersToQuery({ ...EMPTY_FILTERS, priority: ["urgent", "high"], assignee: ["ana", "none"] })).toBe("priority=urgent%2Chigh&assignee=ana&unassigned=true");
    const q = new URLSearchParams(filtersToQuery({ ...EMPTY_FILTERS, due: "overdue" }, new Date(2026, 10, 20)));
    expect(q.get("due_before")).toBe("2026-11-19");
    expect(q.get("state_group")).toBe("backlog,unstarted,started");
  });

  it("computes drop patches and sort keys", () => {
    const g = groupItems([wi("a")], "state", ctx, "manual", true);
    expect(patchForGroup(g.find((x) => x.key === "done")!, wi("a"))).toEqual({ state_id: "done" });
    expect(patchForGroup(g.find((x) => x.key === "todo")!, wi("a"))).toBeNull();
    expect(sortBetween(1000, 2000)).toBe(1500);
    expect(sortBetween(undefined, 1000)).toBe(0);
    expect(sortBetween(1000, undefined)).toBe(2000);
  });

  it("date helpers", () => {
    const g = monthGrid("2026-11");
    expect(g[0]![0]).toBe("2026-10-26");
    expect(g.at(-1)!.at(-1)).toBe("2026-12-06");
    expect(g.every((w) => w.length === 7)).toBe(true);
    expect(addDays("2026-02-28", 1)).toBe("2026-03-01");
    expect(daysBetween("2026-11-20", "2026-12-01")).toBe(11);
  });
});
