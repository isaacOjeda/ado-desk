import { call } from "../../lib/api";

/** Mirrors `work_items::WorkItemDetail` (Rust). Every `*Html` field is already sanitized. */
export interface WorkItemDetail {
  id: number;
  /** Revision loaded; writes send it so a change made elsewhere is not overwritten. */
  rev: number;
  title: string;
  workItemType: string;
  state: string;
  reason: string | null;
  project: string;
  url: string;
  assignedTo: string | null;
  createdBy: string | null;
  createdAt: string | null;
  changedBy: string | null;
  changedAt: string | null;
  priority: number | null;
  severity: string | null;
  areaPath: string | null;
  iterationPath: string | null;
  tags: string[];
  storyPoints: number | null;
  effort: number | null;
  originalEstimate: number | null;
  remainingWork: number | null;
  completedWork: number | null;
  commentCount: number | null;
  descriptionHtml: string | null;
  reproStepsHtml: string | null;
  systemInfoHtml: string | null;
  acceptanceCriteriaHtml: string | null;
  parent: LinkedWorkItem | null;
  children: LinkedWorkItem[];
  related: LinkedWorkItem[];
  development: DevelopmentLink[];
}

export interface LinkedWorkItem {
  id: number;
  /** Link type name, e.g. "Related" or "Successor". */
  link: string;
  title: string;
  workItemType: string;
  state: string;
  url: string;
}

export type DevelopmentKind = "pullRequest" | "commit" | "branch" | "build" | "hyperlink" | "other";

export interface DevelopmentLink {
  kind: DevelopmentKind;
  label: string;
  /** Link type name, e.g. "Fixed in Commit". */
  link: string;
  url: string | null;
}

/** Mirrors `work_items::WorkItemComment` (Rust). `html` is already sanitized. */
export interface WorkItemComment {
  id: number;
  /** Azure DevOps comment version; edits and deletes send the one the user saw. */
  version: number;
  author: string;
  /** Written by the signed-in user. */
  isMine: boolean;
  /** Plain text to edit; `null` when not the user's or it has formatting (delete only). */
  editableText: string | null;
  createdAt: string | null;
  modifiedAt: string | null;
  isEdited: boolean;
  html: string;
}

export const workItemApi = {
  get: (id: number) => call<WorkItemDetail>("get_work_item", { id }),
  /** Newest first, deleted comments removed. */
  comments: (project: string, id: number) => call<WorkItemComment[]>("list_work_item_comments", { project, id }),
};

/** Writes of `work_item_actions.rs`. Rust validates every value and never signs out on failure. */
export const workItemActionsApi = {
  /** `false` only when Azure DevOps refused a write for lack of access (read-only token). */
  writeAccess: (id: number, rev: number) => call<boolean>("work_item_write_access", { id, rev }),
  states: (project: string, workItemType: string) =>
    call<string[]>("list_work_item_states", { project, workItemType }),
  setState: (item: WorkItemDetail, value: string) =>
    call<void>("set_work_item_state", {
      project: item.project,
      workItemType: item.workItemType,
      id: item.id,
      rev: item.rev,
      value,
    }),
  /** `null` unassigns. */
  setAssignee: (item: WorkItemDetail, assignee: string | null) =>
    call<void>("set_work_item_assignee", { project: item.project, id: item.id, rev: item.rev, assignee }),
  addComment: (item: WorkItemDetail, text: string) =>
    call<void>("add_work_item_comment", { project: item.project, id: item.id, text }),
  updateComment: (item: WorkItemDetail, comment: WorkItemComment, text: string) =>
    call<void>("update_work_item_comment", {
      project: item.project,
      id: item.id,
      commentId: comment.id,
      version: comment.version,
      text,
    }),
  deleteComment: (item: WorkItemDetail, comment: WorkItemComment) =>
    call<void>("delete_work_item_comment", {
      project: item.project,
      id: item.id,
      commentId: comment.id,
      version: comment.version,
    }),
};

/** Rust accepts comments of 1 to this many characters. */
export const MAX_COMMENT_CHARS = 4000;
