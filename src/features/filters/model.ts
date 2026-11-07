// Pure filter rules: defaults, normalization, active-filter counting and parsing of stored values.
import type {
  Assignee,
  IterationFilter,
  PullRequestFilter,
  PullRequestRoleFilter,
  PullRequestStatusFilter,
  WorkItemFilter,
  WorkItemStateFilter,
} from "./api";

/** Reproduces the historic list: open work items assigned to me, in every project. */
export const DEFAULT_WORK_ITEM_FILTER: WorkItemFilter = {
  project: null,
  assignee: { kind: "me" },
  iteration: { kind: "any" },
  types: [],
  state: "open",
};

/** Reproduces the historic list: my active pull requests (created or reviewing), in every project. */
export const DEFAULT_PULL_REQUEST_FILTER: PullRequestFilter = {
  project: null,
  status: "active",
  role: "mine",
};

const STATES: readonly WorkItemStateFilter[] = ["open", "closed", "all"];
const STATUSES: readonly PullRequestStatusFilter[] = ["active", "completed", "abandoned", "all"];
const ROLES: readonly PullRequestRoleFilter[] = ["mine", "created", "reviewing", "everyone"];

/** Sprints only exist within a project; a project change also drops a sprint of the old one. */
export function normalizeWorkItemFilter(next: WorkItemFilter, previous?: WorkItemFilter): WorkItemFilter {
  const projectChanged = previous !== undefined && previous.project !== next.project;
  const iteration: IterationFilter =
    !next.project || (projectChanged && next.iteration.kind === "path") ? { kind: "any" } : next.iteration;
  return { ...next, iteration };
}

/** Everyone's pull requests are only listed within one project. */
export function normalizePullRequestFilter(next: PullRequestFilter): PullRequestFilter {
  return !next.project && next.role === "everyone" ? { ...next, role: "mine" } : next;
}

export function countWorkItemFilters(filter: WorkItemFilter): number {
  return [
    filter.project !== null,
    filter.assignee.kind !== "me",
    filter.iteration.kind !== "any",
    filter.types.length > 0,
    filter.state !== "open",
  ].filter(Boolean).length;
}

export function countPullRequestFilters(filter: PullRequestFilter): number {
  return [filter.project !== null, filter.status !== "active", filter.role !== "mine"].filter(Boolean).length;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function text(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value : null;
}

function oneOf<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return allowed.includes(value as T) ? (value as T) : fallback;
}

function parseAssignee(value: unknown): Assignee {
  if (!isRecord(value)) return DEFAULT_WORK_ITEM_FILTER.assignee;
  switch (value.kind) {
    case "me":
    case "anyone":
    case "unassigned":
      return { kind: value.kind };
    case "person": {
      const uniqueName = text(value.uniqueName);
      if (!uniqueName) return DEFAULT_WORK_ITEM_FILTER.assignee;
      const displayName = text(value.displayName) ?? undefined;
      return { kind: "person", uniqueName, displayName };
    }
    default:
      return DEFAULT_WORK_ITEM_FILTER.assignee;
  }
}

function parseIteration(value: unknown): IterationFilter {
  if (!isRecord(value)) return DEFAULT_WORK_ITEM_FILTER.iteration;
  if (value.kind === "current") return { kind: "current" };
  const path = text(value.path);
  return value.kind === "path" && path ? { kind: "path", path } : DEFAULT_WORK_ITEM_FILTER.iteration;
}

/** Validates a stored value field by field; anything unknown falls back to the default. */
export function parseWorkItemFilter(raw: unknown): WorkItemFilter {
  if (!isRecord(raw)) return DEFAULT_WORK_ITEM_FILTER;
  const types = Array.isArray(raw.types) ? raw.types.filter((t): t is string => text(t) !== null) : [];
  return normalizeWorkItemFilter({
    project: text(raw.project),
    assignee: parseAssignee(raw.assignee),
    iteration: parseIteration(raw.iteration),
    types: [...new Set(types)],
    state: oneOf(raw.state, STATES, DEFAULT_WORK_ITEM_FILTER.state),
  });
}

export function parsePullRequestFilter(raw: unknown): PullRequestFilter {
  if (!isRecord(raw)) return DEFAULT_PULL_REQUEST_FILTER;
  return normalizePullRequestFilter({
    project: text(raw.project),
    status: oneOf(raw.status, STATUSES, DEFAULT_PULL_REQUEST_FILTER.status),
    role: oneOf(raw.role, ROLES, DEFAULT_PULL_REQUEST_FILTER.role),
  });
}

export type FilterScope = "workItems" | "pullRequests";

/** Filters are remembered per organization and list on this device. */
export function storageKey(organization: string, scope: FilterScope): string {
  return `ado-desk.filters.${organization.toLowerCase()}.${scope}`;
}
