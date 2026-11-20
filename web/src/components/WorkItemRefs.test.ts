import { describe, expect, it } from "vitest";
import { findRefs, refPattern } from "./WorkItemRefs";

describe("work item references", () => {
  it("finds keys of known projects only", () => {
    const re = refPattern(["WEB", "API"]);
    expect(findRefs("See WEB-12 and API-3, not UTF-8 or WEB-0 or WEB-123x.", re).map((r) => r.key)).toEqual(["WEB-12", "API-3"]);
    expect(findRefs("WEB-12", refPattern([]))).toEqual([]);
  });
  it("ignores identifiers that are not keys", () => {
    expect(refPattern(["we b", "x(y)"])).toBeNull();
  });
});
