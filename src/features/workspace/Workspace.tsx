import { useEffect, useMemo, useState } from "react";
import { api, type ApiError, type PullRequest, type Session, type WorkItem } from "../../lib/api";
import { useResource } from "../../lib/useResource";
import { ErrorBanner, Spinner } from "../../components/ui";
import { Sidebar, type View } from "./Sidebar";
import { WorkItemList } from "./WorkItemList";
import { PullRequestList } from "./PullRequestList";
import { WorkItemDetailView } from "../workitem/WorkItemDetailView";
import { PullRequestDetailView } from "../pullrequest/PullRequestDetailView";

interface Props {
  session: Session;
  onSignOut: () => void;
  onUnauthorized: () => void;
}

type PrFilter = "all" | "reviewer" | "author";

type Selection =
  | { kind: "workItem"; workItem: WorkItem }
  | { kind: "pullRequest"; pullRequest: PullRequest }
  | null;

const TITLES: Record<View, string> = {
  workItems: "My work items",
  pullRequests: "Pull requests",
};

function matches(query: string, ...values: (string | number)[]) {
  const q = query.trim().toLowerCase();
  return !q || values.some((v) => String(v).toLowerCase().includes(q));
}

export function Workspace({ session, onSignOut, onUnauthorized }: Props) {
  const [view, setView] = useState<View>("workItems");
  const [query, setQuery] = useState("");
  const [prFilter, setPrFilter] = useState<PrFilter>("all");
  const [selection, setSelection] = useState<Selection>(null);

  const workItems = useResource(api.listWorkItems);
  const pullRequests = useResource(api.listPullRequests);
  const active = view === "workItems" ? workItems : pullRequests;

  useEffect(() => {
    const failed: (ApiError | null)[] = [workItems.error, pullRequests.error];
    if (failed.some((e) => e?.kind === "unauthorized" || e?.kind === "notSignedIn")) onUnauthorized();
  }, [workItems.error, pullRequests.error, onUnauthorized]);

  // Esc closes a detail view; Cmd/Ctrl+R refreshes the list instead of reloading the webview.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
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
      (pullRequests.data ?? []).filter(
        (pr: PullRequest) =>
          (prFilter === "all" || pr.role === prFilter) &&
          matches(query, pr.id, pr.title, pr.repository, pr.author, pr.sourceBranch),
      ),
    [pullRequests.data, query, prFilter],
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
          {view === "pullRequests" && (
            <div className="segmented" role="tablist">
              {(["all", "reviewer", "author"] as PrFilter[]).map((f) => (
                <button
                  key={f}
                  role="tab"
                  aria-selected={prFilter === f}
                  className={prFilter === f ? "is-active" : ""}
                  onClick={() => setPrFilter(f)}
                >
                  {f === "all" ? "All" : f === "reviewer" ? "Reviewing" : "Created"}
                </button>
              ))}
            </div>
          )}
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

        <section className="panel">
          {active.error && <ErrorBanner message={active.error.message} onRetry={active.reload} />}
          {active.loading && !active.data ? (
            <div className="loading">
              <Spinner />
            </div>
          ) : view === "workItems" ? (
            <WorkItemList items={visibleWorkItems} onSelect={(workItem) => setSelection({ kind: "workItem", workItem })} />
          ) : (
            <PullRequestList
              pullRequests={visiblePullRequests}
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
