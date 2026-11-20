import { describe, expect, it } from "vitest";
import type { Page } from "../api/types";
import { flattenTree } from "./pages";

const pg = (id: string, parent: string | null, order: number): Page => ({
  id, project_id: "p", parent_id: parent, owner_id: "u", title: id, access: "project", sort_order: order,
  locked_at: null, created_at: "", updated_at: "", updated_by: null, archived_at: null, can_edit: true, can_manage: true,
});

describe("page tree", () => {
  it("orders parents before children, with depth", () => {
    const t = flattenTree([pg("b", null, 2), pg("a1", "a", 1), pg("a", null, 1), pg("orphan", "gone", 3)]);
    expect(t.map(([p, d]) => `${p.id}:${d}`)).toEqual(["a:0", "a1:1", "b:0", "orphan:0"]);
  });
});

import { isWithin, planMove, selectionTitle } from "./pages";

describe("moving pages", () => {
  const pages = [pg("a", null, 1000), pg("b", null, 2000), pg("c", null, 3000), pg("a1", "a", 1000)];
  it("places before, after and inside", () => {
    expect(planMove(pages, "c", "b", "before")).toEqual({ parent_id: null, sort_order: 1500 });
    expect(planMove(pages, "a", "c", "after")).toEqual({ parent_id: null, sort_order: 4000 });
    expect(planMove(pages, "a", "b", "before")).toEqual({ parent_id: null, sort_order: 1000 });
    expect(planMove(pages, "c", "a", "inside")).toEqual({ parent_id: "a", sort_order: 2000 });
    expect(planMove(pages, "a1", "b", "after")).toEqual({ parent_id: null, sort_order: 2500 });
  });
  it("refuses to move a page into itself or its children", () => {
    expect(planMove(pages, "a", "a1", "inside")).toBeNull();
    expect(planMove(pages, "a", "a", "after")).toBeNull();
    expect(isWithin(pages, "a1", "a")).toBe(true);
    expect(isWithin(pages, "b", "a")).toBe(false);
  });
  it("makes a title from a selection", () => {
    expect(selectionTitle("  Fix\n the   login ")).toBe("Fix the login");
    expect(selectionTitle("x".repeat(300)).length).toBe(255);
  });
});

import type { IntakeEntry } from "../api/types";
import { mayEditIntake } from "./intake";

describe("intake edit rule", () => {
  const e = (status: IntakeEntry["status"], by = "me") => ({ status, created_by: by }) as IntakeEntry;
  it("lets submitters edit until triage and admins until acceptance", () => {
    expect(mayEditIntake(e("pending"), "me", false, false)).toBe(true);
    expect(mayEditIntake(e("declined"), "me", false, false)).toBe(false);
    expect(mayEditIntake(e("pending", "other"), "me", false, false)).toBe(false);
    expect(mayEditIntake(e("declined", "other"), "me", true, false)).toBe(true);
    expect(mayEditIntake(e("accepted", "other"), "me", true, false)).toBe(false);
    expect(mayEditIntake(e("pending"), "me", true, true)).toBe(false);
  });
});
