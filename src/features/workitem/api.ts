import { call } from "../../lib/api";

/** Mirrors `work_items::WorkItemDetail` (Rust). Every `*Html` field is already sanitized. */
export interface WorkItemDetail {
  id: number;
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
  author: string;
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
