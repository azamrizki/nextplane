import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { CLIENT_ID } from "../api/client";
import { applyEvent } from "./realtime";

describe("realtime", () => {
  it("invalidates the project's lists and the item's details", () => {
    const qc = new QueryClient();
    const spy = vi.spyOn(qc, "invalidateQueries");
    applyEvent(qc, "acme", { workspace: "acme", project_id: "p1", work_item_id: "w1", area: "comment", client: "other" });
    const keys = spy.mock.calls.map((c) => JSON.stringify(c[0]?.queryKey));
    expect(keys).toContain(JSON.stringify(["work-items", "acme", "p1"]));
    expect(keys).toContain(JSON.stringify(["comments", "w1"]));
  });

  it("skips its own echo except for the inbox count", () => {
    const qc = new QueryClient();
    const spy = vi.spyOn(qc, "invalidateQueries");
    applyEvent(qc, "acme", { workspace: "acme", project_id: "p1", work_item_id: null, area: "work_item", client: CLIENT_ID });
    expect(spy.mock.calls.map((c) => JSON.stringify(c[0]?.queryKey))).toEqual([JSON.stringify(["notifications", "acme"])]);
  });

  it("refreshes only pages for page events", () => {
    const qc = new QueryClient();
    const spy = vi.spyOn(qc, "invalidateQueries");
    applyEvent(qc, "acme", { workspace: "acme", project_id: "p1", work_item_id: null, area: "page", client: "other" });
    const keys = spy.mock.calls.map((c) => JSON.stringify(c[0]?.queryKey));
    expect(keys).toContain(JSON.stringify(["pages", "acme", "p1"]));
    expect(keys).not.toContain(JSON.stringify(["work-items", "acme", "p1"]));
  });
});
