import { describe, expect, it } from "vitest";
import { assignableRoles, canRemove, invitableRoles, slugify } from "./perm";

describe("client permission mirror", () => {
  it("matches PERMISSIONS.md D1", () => {
    expect(invitableRoles("admin")).toEqual(["member", "guest"]);
    expect(invitableRoles("member")).toEqual([]);
    expect(assignableRoles("admin", "admin", false)).toEqual([]);
    expect(assignableRoles("admin", "guest", false)).toEqual(["member", "guest"]);
    expect(canRemove("admin", "admin", false)).toBe(false);
    expect(canRemove("guest", "guest", true)).toBe(true);
  });

  it("slugifies like the server", () => {
    expect(slugify("Acme Corp!")).toBe("acme-corp");
    expect(slugify("  --Tim  Dev 2026-- ")).toBe("tim-dev-2026");
  });
});
