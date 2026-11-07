import { call, type PullRequest, type WorkItem } from "../../lib/api";

/** Mirrors `queries::Assignee` (Rust). `displayName` is UI-only and never sent. */
export type Assignee =
  | { kind: "me" }
  | { kind: "anyone" }
  | { kind: "unassigned" }
  | { kind: "person"; uniqueName: string; displayName?: string };

/** Mirrors `queries::IterationFilter`. */
export type IterationFilter = { kind: "any" } | { kind: "current" } | { kind: "path"; path: string };

export type WorkItemStateFilter = "open" | "closed" | "all";

/** Mirrors `queries::WorkItemFilter`. */
export interface WorkItemFilter {
  project: string | null;
  assignee: Assignee;
  iteration: IterationFilter;
  types: string[];
  state: WorkItemStateFilter;
}

export type PullRequestStatusFilter = "active" | "completed" | "abandoned" | "all";
export type PullRequestRoleFilter = "mine" | "created" | "reviewing" | "everyone";

/** Mirrors `queries::PullRequestFilter`. */
export interface PullRequestFilter {
  project: string | null;
  status: PullRequestStatusFilter;
  role: PullRequestRoleFilter;
}

/** Mirrors `model::PullRequestState`. Drafts are `active` with `isDraft`. */
export type PullRequestState = "active" | "completed" | "abandoned";

/** A pull request from `query_pull_requests`, which also reports its lifecycle state. */
export interface FilteredPullRequest extends PullRequest {
  status: PullRequestState;
  closedAt: string | null;
}

/** Mirrors `queries::Iteration`. */
export interface Iteration {
  id: string;
  name: string;
  path: string;
  startDate: string | null;
  finishDate: string | null;
  isCurrent: boolean;
}

/** Mirrors `queries::TeamMember`. */
export interface TeamMember {
  displayName: string;
  uniqueName: string;
}

function wireAssignee(assignee: Assignee): Assignee {
  return assignee.kind === "person" ? { kind: "person", uniqueName: assignee.uniqueName } : assignee;
}

export const filtersApi = {
  queryWorkItems: (filter: WorkItemFilter) =>
    call<WorkItem[]>("query_work_items", { filter: { ...filter, assignee: wireAssignee(filter.assignee) } }),
  queryPullRequests: (filter: PullRequestFilter) => call<FilteredPullRequest[]>("query_pull_requests", { filter }),
  listProjects: () => call<string[]>("list_projects"),
  listIterations: (project: string) => call<Iteration[]>("list_iterations", { project }),
  listWorkItemTypes: (project: string | null) => call<string[]>("list_work_item_types", { project }),
  listTeamMembers: (project: string) => call<TeamMember[]>("list_team_members", { project }),
};
