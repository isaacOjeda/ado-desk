import { useCallback, useEffect, useMemo, useState } from "react";
import type { ApiError, Session, WorkItem } from "../../lib/api";
import { settingsApi, type SavedView, type ViewScope } from "../../lib/settings";
import { useResource } from "../../lib/useResource";
import { MOD_KEY } from "../../lib/platform";
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
import type { SearchHit } from "../search/api";
import { recordRecent } from "../search/recents";
import { requestQuickSearch } from "../search/events";
import { InboxView } from "../inbox/InboxView";
import { inboxApi, useInbox } from "../inbox/api";
import { InboxFilterBar } from "../inbox/InboxFilterBar";
import { useInboxFilter } from "../inbox/filter";
import { activeViewId, viewsFor } from "../views/model";
import { useSavedViews } from "../views/useSavedViews";
import { useCheckSummaries } from "../pullrequest/checks";
import { AssistantPanel } from "../assistant/AssistantPanel";

interface Props {
  session: Session;
  onSignOut: () => void;
  onUnauthorized: () => void;
  /** An item to open (from the quick search); handled once its organization is active. */
  openRequest: SearchHit | null;
  onOpenHandled: () => void;
}

type Selection =
  | { kind: "workItem"; workItem: WorkItem }
  | { kind: "pullRequest"; pullRequest: FilteredPullRequest }
  | null;

const TITLES: Record<View, string> = {
  inbox: "Inbox",
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

function hitOf(organization: string, selection: NonNullable<Selection>): SearchHit {
  return selection.kind === "workItem"
    ? { kind: "workItem", organization, item: selection.workItem }
    : { kind: "pullRequest", organization, item: selection.pullRequest };
}

export function Workspace({ session, onSignOut, onUnauthorized, openRequest, onOpenHandled }: Props) {
  const [view, setView] = useState<View>("workItems");
  const [query, setQuery] = useState("");
  const [selection, setSelectionState] = useState<Selection>(null);
  // The panel mounts on first open and then stays mounted, so hiding it keeps the session.
  const [assistant, setAssistant] = useState<"never" | "open" | "hidden">("never");
  const toggleAssistant = useCallback(() => setAssistant((current) => (current === "open" ? "hidden" : "open")), []);
  const organization = session.organization;

  // Every opened item becomes a quick search "recent".
  const setSelection = useCallback(
    (next: Selection) => {
      if (next) recordRecent(hitOf(organization, next));
      setSelectionState(next);
    },
    [organization],
  );

  useEffect(() => {
    if (!openRequest || openRequest.organization !== organization) return;
    // Items opened from the Inbox return to it on Back.
    if (openRequest.kind === "workItem") {
      setView((current) => (current === "inbox" ? current : "workItems"));
      setSelection({ kind: "workItem", workItem: openRequest.item });
    } else {
      setView((current) => (current === "inbox" ? current : "pullRequests"));
      setSelection({ kind: "pullRequest", pullRequest: openRequest.item });
    }
    onOpenHandled();
  }, [openRequest, organization, setSelection, onOpenHandled]);
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
  // Dots for active pull requests, loaded after each list fetch.
  const checkSummaries = useCheckSummaries(organization, pullRequests.data);
  const [allViews, acceptViews] = useSavedViews();
  const [viewError, setViewError] = useState<string | null>(null);
  const views = useMemo(() => viewsFor(allViews ?? [], organization), [allViews, organization]);
  const currentViewId = useMemo(
    () =>
      activeViewId(
        views,
        { workItems: workItemFilter, pullRequests: pullRequestFilter },
        view === "inbox" || selection ? null : view,
      ),
    [views, workItemFilter, pullRequestFilter, view, selection],
  );

  // A view applies its filters through the list's persistent filter, so the list keeps them.
  const applyView = (saved: SavedView) => {
    setViewError(null);
    setSelection(null);
    if (saved.scope === "workItems") setWorkItemFilter(parseWorkItemFilter(saved.filter));
    else setPullRequestFilter(parsePullRequestFilter(saved.filter));
    setView(saved.scope);
  };

  const saveView = (scope: ViewScope, filter: unknown) => async (name: string) => {
    acceptViews(await settingsApi.saveView(organization, scope, name, filter));
  };

  const removeView = (saved: SavedView) => {
    setViewError(null);
    settingsApi
      .deleteView(saved.id)
      .then(acceptViews)
      .catch((err: unknown) =>
        setViewError(`Could not remove the view: ${err instanceof Error ? err.message : "unknown error"}`),
      );
  };
  const active = view === "workItems" ? workItems : pullRequests;
  const inbox = useInbox();
  const [inboxFilter, setInboxFilter] = useInboxFilter();
  const unread = inbox?.filter((item) => !item.read).length ?? 0;

  useEffect(() => {
    const failed: (ApiError | null)[] = [workItems.error, pullRequests.error];
    if (failed.some((e) => e?.kind === "unauthorized" || e?.kind === "notSignedIn")) onUnauthorized();
  }, [workItems.error, pullRequests.error, onUnauthorized]);

  // Esc closes a detail view; Cmd/Ctrl+R refreshes the list instead of reloading the webview;
  // Cmd/Ctrl+, opens Settings.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "j") {
        e.preventDefault();
        toggleAssistant();
        return;
      }
      // Keys typed in the assistant's terminal belong to it (Esc interrupts Claude there).
      if (e.target instanceof Element && e.target.closest(".assistant-panel")) return;
      if ((e.metaKey || e.ctrlKey) && e.key === ",") {
        e.preventDefault();
        openSettings();
        return;
      }
      if (e.key === "Escape" && selection) {
        // Leaving a comment box must not close the detail and drop what was typed.
        if (e.target instanceof HTMLTextAreaElement) {
          e.target.blur();
          return;
        }
        setSelection(null);
        return;
      }
      if (!selection && (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "r") {
        e.preventDefault();
        if (view === "inbox") void inboxApi.checkNow();
        else active.reload();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, selection, setSelection, view, toggleAssistant]);

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
          inbox: unread,
          workItems: workItems.data?.length ?? null,
          pullRequests: pullRequests.data?.length ?? null,
        }}
        onSelect={(next) => {
          setViewError(null);
          setSelection(null);
          setView(next);
        }}
        onSignOut={onSignOut}
        onOpenSettings={openSettings}
        onSearch={requestQuickSearch}
        assistantOpen={assistant === "open"}
        onToggleAssistant={toggleAssistant}
        views={views}
        activeViewId={currentViewId}
        onApplyView={applyView}
        onRemoveView={removeView}
      />

      <main className="content">
        <div className="content-main">
        {selection?.kind === "workItem" ? (
          <WorkItemDetailView
            key={selection.workItem.id}
            workItem={selection.workItem}
            onBack={() => setSelection(null)}
            me={session.email ? { email: session.email, displayName: session.displayName } : null}
          />
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
          {view === "inbox" ? (
            <>
              <button className="btn btn-ghost btn-sm" onClick={() => void inboxApi.markRead()} disabled={unread === 0}>
                Mark all read
              </button>
              <button className="btn btn-ghost" onClick={() => void inboxApi.checkNow()} title="Check now">
                ↻
              </button>
            </>
          ) : (
            <button className="btn btn-ghost" onClick={active.reload} disabled={active.loading} title={`Refresh (${MOD_KEY}R)`}>
              {active.loading ? <Spinner /> : "↻"}
            </button>
          )}
        </header>

        {view === "inbox" ? (
          <InboxFilterBar filter={inboxFilter} onChange={setInboxFilter} items={inbox} />
        ) : view === "workItems" ? (
          <WorkItemFilterBar
            key={organization}
            filter={workItemFilter}
            onChange={setWorkItemFilter}
            projects={projects.data}
            onSaveView={saveView("workItems", workItemFilter)}
          />
        ) : (
          <PullRequestFilterBar
            filter={pullRequestFilter}
            onChange={setPullRequestFilter}
            projects={projects.data}
            onSaveView={saveView("pullRequests", pullRequestFilter)}
          />
        )}

        <section className="panel">
          {viewError && <ErrorBanner message={viewError} />}
          {view !== "inbox" && active.error && <ErrorBanner message={active.error.message} onRetry={active.reload} />}
          {view === "inbox" ? (
            <InboxView items={inbox} query={query} filter={inboxFilter} />
          ) : active.loading && !active.data ? (
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
              checks={checkSummaries}
            />
          )}
        </section>
          </>
        )}
        </div>
        {assistant !== "never" && (
          <AssistantPanel
            open={assistant === "open"}
            onClose={() => setAssistant("hidden")}
            onOpenSettings={openSettings}
          />
        )}
      </main>
    </div>
  );
}
