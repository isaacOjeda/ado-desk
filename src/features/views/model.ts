// Pure rules for saved views: which views an organization sees and when a view is the active one.
import type { SavedView } from "../../lib/settings";
import type { PullRequestFilter, WorkItemFilter } from "../filters/api";
import { parsePullRequestFilter, parseWorkItemFilter } from "../filters/model";

/** Views of `organization` plus the starters, in stored order. */
export function viewsFor(views: readonly SavedView[], organization: string): SavedView[] {
  const org = organization.toLowerCase();
  return views.filter((view) => view.organization === null || view.organization === org);
}

/** Identity of a work item filter; ignores the UI-only `displayName` and the order of types. */
export function workItemFilterKey(filter: WorkItemFilter): string {
  const { project, assignee, iteration, types, state } = filter;
  return JSON.stringify([
    project,
    assignee.kind,
    assignee.kind === "person" ? assignee.uniqueName : null,
    iteration.kind,
    iteration.kind === "path" ? iteration.path : null,
    [...types].sort(),
    state,
  ]);
}

export function pullRequestFilterKey(filter: PullRequestFilter): string {
  return JSON.stringify([filter.project, filter.status, filter.role]);
}

/** The view whose filters equal the current ones of its list, if any. */
export function activeViewId(
  views: readonly SavedView[],
  current: { workItems: WorkItemFilter; pullRequests: PullRequestFilter },
  list: SavedView["scope"] | null,
): string | null {
  if (!list) return null;
  const key = list === "workItems" ? workItemFilterKey(current.workItems) : pullRequestFilterKey(current.pullRequests);
  const match = views.find(
    (view) =>
      view.scope === list &&
      (list === "workItems"
        ? workItemFilterKey(parseWorkItemFilter(view.filter))
        : pullRequestFilterKey(parsePullRequestFilter(view.filter))) === key,
  );
  return match?.id ?? null;
}
