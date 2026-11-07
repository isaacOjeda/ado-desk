import { useCallback, useEffect, useMemo, useState } from "react";
import type { ApiError, Session, WorkItem } from "../../lib/api";
import { settingsApi } from "../../lib/settings";
import { useResource } from "../../lib/useResource";
import { ErrorBanner, Spinner } from "../../components/ui";
import { Sidebar, type View } from "./Sidebar";
import { WorkItemList } from "./WorkItemList";
import { PullRequestList } from "./PullRequestList";
import { WorkItemDetailView } from "../workitem/WorkItemDetailView";
import { PullRequestDetailView } from "../pullrequest/PullRequestDetailView";
import { filtersApi, type FilteredPullRequest } from "../filters/api";
import {
  countPullRequestFilters,
  countWorkItemFilters,
  parsePullRequestFilter,
  parseWorkItemFilter,
} from "../filters/model";
import { usePersistentFilter } from "../filters/usePersistentFilter";
import { WorkItemFilterBar } from "../filters/WorkItemFilterBar";
import { PullRequestFilterBar } from "../filters/PullRequestFilterBar";

interface Props {
  session: Session;
  onSignOut: () => void;
  onUnauthorized: () => void;
}

type Selection =
  | { kind: "workItem"; workItem: WorkItem }
  | { kind: "pullRequest"; pullRequest: FilteredPullRequest }
  | null;

const TITLES: Record<View, string> = {
  workItems: "My work items",
  pullRequests: "Pull requests",
};

function openSettings() {
  settingsApi.openWindow().catch((err: unknown) => console.error("Could not open settings", err));
}

function matches(query: string, ...values: (string | number)[]) {
  const q = query.trim().toLowerCase();
  return !q || values.some((v) => String(v).toLowerCase().includes(q));
}

export function Workspace({ session, onSignOut, onUnauthorized }: Props) {
  const [view, setView] = useState<View>("workItems");
  const [query, setQuery] = useState("");
  const [selection, setSelection] = useState<Selection>(null);
  const organization = session.organization;
  const [workItemFilter, setWorkItemFilter] = usePersistentFilter(organization, "workItems", parseWorkItemFilter);
  const [pullRequestFilter, setPullRequestFilter] = usePersistentFilter(
    organization,
    "pullRequests",
    parsePullRequestFilter,
  );

  // `organization` is a dependency so switching organizations refetches even with equal filters.
  const workItems = useResource(
    useCallback(() => filtersApi.queryWorkItems(workItemFilter), [organization, workItemFilter]),
  );
  const pullRequests = useResource(
    useCallback(() => filtersApi.queryPullRequests(pullRequestFilter), [organization, pullRequestFilter]),
  );
  const projects = useResource(useCallback(() => filtersApi.listProjects(), [organization]));
  const active = view === "workItems" ? workItems : pullRequests;

  useEffect(() => {
    const failed: (ApiError | null)[] = [workItems.error, pullRequests.error];
    if (failed.some((e) => e?.kind === "unauthorized" || e?.kind === "notSignedIn")) onUnauthorized();
  }, [workItems.error, pullRequests.error, onUnauthorized]);

  // Esc closes a detail view; Cmd/Ctrl+R refreshes the list instead of reloading the webview;
  // Cmd/Ctrl+, opens Settings.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === ",") {
        e.preventDefault();
        openSettings();
        return;
      }
      if (e.key === "Escape" && selection) {
        setSelection(null);
        return;
      }
      if (!selection && (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "r") {
        e.preventDefault();
        active.reload();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, selection]);

  const visibleWorkItems = useMemo(
    () =>
      (workItems.data ?? []).filter((w) =>
        matches(query, w.id, w.title, w.state, w.project, w.workItemType),
      ),
    [workItems.data, query],
  );

  const visiblePullRequests = useMemo(
    () =>
      (pullRequests.data ?? []).filter((pr) =>
        matches(query, pr.id, pr.title, pr.repository, pr.author, pr.sourceBranch),
      ),
    [pullRequests.data, query],
  );


  return (
    <div className="shell">
      <Sidebar
        session={session}
        view={view}
        counts={{
          workItems: workItems.data?.length ?? null,
          pullRequests: pullRequests.data?.length ?? null,
        }}
        onSelect={(next) => {
          setSelection(null);
          setView(next);
        }}
        onSignOut={onSignOut}
        onOpenSettings={openSettings}
      />

      <main className="content">
        {selection?.kind === "workItem" ? (
          <WorkItemDetailView key={selection.workItem.id} workItem={selection.workItem} onBack={() => setSelection(null)} />
        ) : selection?.kind === "pullRequest" ? (
          <PullRequestDetailView
            key={selection.pullRequest.id}
            pullRequest={selection.pullRequest}
            onBack={() => setSelection(null)}
          />
        ) : (
          <>
        <header className="toolbar" data-tauri-drag-region>
          <h2>{TITLES[view]}</h2>
          <input
            className="search"
            placeholder="Filter…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button className="btn btn-ghost" onClick={active.reload} disabled={active.loading} title="Refresh (⌘R)">
            {active.loading ? <Spinner /> : "↻"}
          </button>
        </header>

        {view === "workItems" ? (
          <WorkItemFilterBar
            key={organization}
            filter={workItemFilter}
            onChange={setWorkItemFilter}
            projects={projects.data}
          />
        ) : (
          <PullRequestFilterBar
            filter={pullRequestFilter}
            onChange={setPullRequestFilter}
            projects={projects.data}
          />
        )}

        <section className="panel">
          {active.error && <ErrorBanner message={active.error.message} onRetry={active.reload} />}
          {active.loading && !active.data ? (
            <div className="loading">
              <Spinner />
            </div>
          ) : view === "workItems" ? (
            <WorkItemList
              items={visibleWorkItems}
              isFiltered={countWorkItemFilters(workItemFilter) > 0 || query.trim() !== ""}
              onSelect={(workItem) => setSelection({ kind: "workItem", workItem })}
            />
          ) : (
            <PullRequestList
              pullRequests={visiblePullRequests}
              isFiltered={countPullRequestFilters(pullRequestFilter) > 0 || query.trim() !== ""}
              onSelect={(pullRequest) => setSelection({ kind: "pullRequest", pullRequest })}
            />
          )}
        </section>
          </>
        )}
      </main>
    </div>
  );
}
