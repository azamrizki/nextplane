export interface User {
  id: string;
  email: string;
  name: string;
  avatar: string | null;
  is_instance_admin: boolean;
  is_bot: boolean;
  created_at: string;
}

export interface InstanceInfo {
  name: string;
  setup_required: boolean;
}

export interface VersionInfo {
  version: string;
  commit: string;
  build_date: string;
}

export type WsRole = "owner" | "admin" | "member" | "guest";
export type SignupPolicy = "closed" | "invite" | "open";

export interface InstanceInfoFull extends InstanceInfo {
  signup_policy: SignupPolicy;
  email_enabled: boolean;
  sso_name?: string | null;
  password_enabled?: boolean;
  providers?: { key: string; name: string }[];
  ai_enabled?: boolean;
  unsplash_enabled?: boolean;
}

export interface Workspace {
  id: string;
  slug: string;
  name: string;
  logo: string | null;
  members_can_create_projects: boolean;
  created_at: string;
  deleted_at: string | null;
}

export interface MyWorkspace extends Workspace {
  role: WsRole;
}

export interface WsMember {
  user_id: string;
  email: string;
  name: string;
  avatar: string | null;
  role: WsRole;
  joined_at: string;
}

export interface Invite {
  id: string;
  email: string;
  role: WsRole;
  invited_by: string;
  expires_at: string;
  created_at: string;
}

export interface InviteResult {
  email: string;
  role: WsRole;
  link?: string;
  emailed: boolean;
  error?: string;
}

export interface InviteInfo {
  id: string;
  workspace_slug: string;
  workspace_name: string;
  email: string;
  role: WsRole;
  invited_by_name: string;
  account_exists: boolean;
  signup_allowed: boolean;
}

export interface AdminSettings {
  name: string;
  signup_policy: SignupPolicy;
  email_enabled: boolean;
  public_url: string | null;
}

export interface AdminWorkspace extends Workspace {
  member_count: number;
}

export type ProjRole = "admin" | "contributor" | "commenter" | "guest";
export type StateGroup = "backlog" | "unstarted" | "started" | "completed" | "cancelled";
export const STATE_GROUPS: StateGroup[] = ["backlog", "unstarted", "started", "completed", "cancelled"];
export type EstimateType = "none" | "points" | "tshirt";

export interface Project {
  id: string;
  workspace_id: string;
  identifier: string;
  name: string;
  description: string;
  icon: string | null;
  cover_color: string | null;
  /** An uploaded image or an Unsplash photo, with the photographer's credit for Unsplash. */
  cover_image?: string | null;
  cover_credit?: { name: string; url: string } | null;
  /** It has a public board. */
  is_published?: boolean;
  lead_id: string | null;
  default_assignee_id: string | null;
  is_private: boolean;
  estimate_type: EstimateType;
  estimate_values: string[];
  feature_cycles: boolean;
  feature_modules: boolean;
  feature_pages: boolean;
  feature_intake: boolean;
  created_by: string;
  created_at: string;
  archived_at: string | null;
  /** Automations, in months (0 = off). */
  archive_in: number;
  close_in: number;
  close_state_id: string | null;
  member_role: ProjRole | null;
  member_count: number;
  my_role: ProjRole | null;
  is_member: boolean;
}

export interface ProjMember {
  user_id: string;
  email: string;
  name: string;
  avatar: string | null;
  role: ProjRole;
  workspace_role: WsRole;
}

export interface WfState {
  id: string;
  name: string;
  color: string;
  group: StateGroup;
  sort_order: number;
  is_default: boolean;
}

export interface Label {
  id: string;
  parent_id: string | null;
  name: string;
  color: string;
  sort_order: number;
}

export type Priority = "urgent" | "high" | "medium" | "low" | "none";
export const PRIORITIES: Priority[] = ["urgent", "high", "medium", "low", "none"];

export interface WorkItem {
  id: string;
  project_id: string;
  sequence: number;
  title: string;
  description_json?: unknown;
  description_html?: string;
  state_id: string;
  priority: Priority;
  start_date: string | null;
  due_date: string | null;
  estimate: string | null;
  parent_id: string | null;
  cycle_id?: string | null;
  module_ids?: string[];
  is_draft?: boolean;
  in_intake?: boolean;
  sort_order: number;
  created_by: string;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
  archived_at: string | null;
  assignee_ids: string[];
  label_ids: string[];
  sub_item_count: number;
}

export interface Activity {
  id: string;
  actor_id: string;
  actor_name: string;
  actor_is_bot: boolean;
  verb: string;
  field: string | null;
  old_value: string | null;
  new_value: string | null;
  created_at: string;
}

export type RelationKind = "blocks" | "blocked_by" | "relates_to" | "duplicate_of" | "duplicated_by";

export interface Relation {
  id: string;
  kind: RelationKind;
  item: { id: string; project_id: string; sequence: number; title: string; state_id: string; identifier: string };
}

export type CycleStatus = "current" | "upcoming" | "completed" | "draft";

export interface Cycle {
  id: string;
  project_id: string;
  name: string;
  description: string;
  start_date: string | null;
  end_date: string | null;
  owner_id: string | null;
  created_by: string;
  created_at: string;
  archived_at: string | null;
  status: CycleStatus;
  total_items: number;
  completed_items: number;
}

export interface CycleProgress {
  groups: { group: StateGroup; items: number; points: number }[];
  total_items: number;
  completed_items: number;
  total_points: number;
  completed_points: number;
  burndown: { date: string; ideal: number; remaining: number | null }[];
}

export type ModuleStatus = "backlog" | "planned" | "in_progress" | "paused" | "completed" | "cancelled";
export const MODULE_STATUSES: ModuleStatus[] = ["backlog", "planned", "in_progress", "paused", "completed", "cancelled"];

export interface Module {
  id: string;
  project_id: string;
  name: string;
  description: string;
  status: ModuleStatus;
  lead_id: string | null;
  start_date: string | null;
  target_date: string | null;
  created_by: string;
  created_at: string;
  archived_at: string | null;
  total_items: number;
  completed_items: number;
  member_ids: string[];
}

export interface Notification {
  id: string;
  project_id: string;
  work_item_id: string;
  actor_id: string;
  kind: "assigned" | "mentioned" | "commented" | "updated" | "archived";
  field: string | null;
  old_value: string | null;
  new_value: string | null;
  comment_id: string | null;
  created_at: string;
  read_at: string | null;
  archived_at: string | null;
  snoozed_until: string | null;
  identifier: string;
  sequence: number;
  title: string;
}

export interface Page {
  id: string;
  project_id: string;
  parent_id: string | null;
  owner_id: string;
  title: string;
  access: "project" | "private";
  sort_order: number;
  locked_at: string | null;
  created_at: string;
  updated_at: string;
  updated_by: string | null;
  archived_at: string | null;
  html?: string;
  has_doc?: boolean;
  can_edit: boolean;
  can_manage: boolean;
}

export interface PageVersion {
  id: string;
  title: string;
  created_by: string | null;
  created_at: string;
  html?: string;
}

export type IntakeStatus = "pending" | "accepted" | "declined" | "duplicate" | "snoozed";

export interface IntakeEntry {
  work_item_id: string;
  sequence: number;
  title: string;
  priority: string;
  created_by: string;
  created_at: string;
  status: IntakeStatus;
  snoozed_until: string | null;
  duplicate_of: string | null;
  duplicate_sequence: number | null;
  reason: string | null;
  source: "app" | "form";
  submitter_name: string | null;
  submitter_email: string | null;
  triaged_by: string | null;
  triaged_at: string | null;
  description_html?: string;
}
