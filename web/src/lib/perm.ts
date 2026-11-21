// Client-side mirror of crates/np-core/src/perm.rs, used only to hide controls a person
// cannot use. The server checks every action again.
import type { MyWorkspace, WsRole } from "../api/types";

const LEVEL: Record<WsRole, number> = { guest: 0, member: 1, admin: 2, owner: 3 };
export const atLeast = (r: WsRole, min: WsRole) => LEVEL[r] >= LEVEL[min];
export const isWsAdmin = (r: WsRole) => atLeast(r, "admin");

export const can = {
  editWorkspace: (w: MyWorkspace) => isWsAdmin(w.role),
  deleteWorkspace: (w: MyWorkspace) => w.role === "owner",
  invite: (w: MyWorkspace) => isWsAdmin(w.role),
  viewMembers: (w: MyWorkspace) => atLeast(w.role, "member"),
  createProject: (w: MyWorkspace) => isWsAdmin(w.role) || (w.role === "member" && w.members_can_create_projects),
};

/** Roles `actor` may invite with (PERMISSIONS D1). */
export function invitableRoles(actor: WsRole): WsRole[] {
  if (actor === "owner") return ["admin", "member", "guest"];
  if (actor === "admin") return ["member", "guest"];
  return [];
}

/** Roles `actor` may set on someone who has role `target` (empty = no control). */
export function assignableRoles(actor: WsRole, target: WsRole, isSelf: boolean): WsRole[] {
  if (actor === "owner") return ["owner", "admin", "member", "guest"];
  if (actor === "admin" && !isSelf && LEVEL[target] <= LEVEL.member) return ["member", "guest"];
  return [];
}

export function canRemove(actor: WsRole, target: WsRole, isSelf: boolean): boolean {
  if (isSelf) return true;
  if (actor === "owner") return true;
  return actor === "admin" && LEVEL[target] <= LEVEL.member;
}

export function slugify(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48)
    .replace(/-+$/g, "");
}

import type { Project, ProjRole } from "../api/types";

const PLEVEL: Record<ProjRole, number> = { guest: 0, commenter: 1, contributor: 2, admin: 3 };
export const projAtLeast = (r: ProjRole | null | undefined, min: ProjRole) => !!r && PLEVEL[r] >= PLEVEL[min];

export const pcan = {
  editSettings: (p: Project) => p.my_role === "admin",
  manageMembers: (p: Project) => p.my_role === "admin",
  createWorkItem: (p: Project) => projAtLeast(p.my_role, "contributor"),
  editWorkItem: (p: Project) => projAtLeast(p.my_role, "contributor"),
  deleteWorkItem: (p: Project, own: boolean) => p.my_role === "admin" || (p.my_role === "contributor" && own),
  comment: (p: Project) => projAtLeast(p.my_role, "commenter"),
  viewWorkItems: (p: Project) => projAtLeast(p.my_role, "commenter"),
  /** The project's archives are for contributors and admins (as in Plane). */
  viewArchives: (p: Project) => projAtLeast(p.my_role, "contributor"),
};

/** Project roles an admin may give someone with workspace role `ws` (guest ceiling). */
export function projRolesFor(ws: WsRole): ProjRole[] {
  return ws === "guest" ? ["commenter", "guest"] : ["admin", "contributor", "commenter", "guest"];
}

export function suggestIdentifier(name: string): string {
  const words = name.split(/[^A-Za-z0-9]+/).filter(Boolean);
  let s = words.length >= 2 ? words.map((w) => w[0]).slice(0, 5).join("") : (words[0] ?? "").slice(0, 3);
  s = s.toUpperCase();
  return /^[A-Z]/.test(s) ? s : ("P" + s).slice(0, 12);
}
