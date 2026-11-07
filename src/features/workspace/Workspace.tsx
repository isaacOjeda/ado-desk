import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type ApiError, type PullRequest, type Session } from "../../lib/api";
import { useResource } from "../../lib/useResource";
import { ErrorBanner, Spinner } from "../../components/ui";
import { Sidebar, type View } from "./Sidebar";
import { WorkItemList } from "./WorkItemList";
import { PullRequestList } from "./PullRequestList";

interface Props {
  session: Session;
  onSignOut: () => void;
  onUnauthorized: () => void;
}

type PrFilter = "all" | "reviewer" | "author";

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

  const workItems = useResource(api.listWorkItems);
  const pullRequests = useResource(api.listPullRequests);
  const active = view === "workItems" ? workItems : pullRequests;

  useEffect(() => {
    const failed: (ApiError | null)[] = [workItems.error, pullRequests.error];
    if (failed.some((e) => e?.kind === "unauthorized" || e?.kind === "notSignedIn")) onUnauthorized();
  }, [workItems.error, pullRequests.error, onUnauthorized]);

  // Cmd/Ctrl+R refreshes the current view instead of reloading the webview.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "r") {
        e.preventDefault();
        active.reload();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);

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

  const open = (url: string) => void openUrl(url);

  return (
    <div className="shell">
      <Sidebar
        session={session}
        view={view}
        counts={{
          workItems: workItems.data?.length ?? null,
          pullRequests: pullRequests.data?.length ?? null,
        }}
        onSelect={setView}
        onSignOut={onSignOut}
      />

      <main className="content">
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
            <WorkItemList items={visibleWorkItems} onOpen={open} />
          ) : (
            <PullRequestList pullRequests={visiblePullRequests} onOpen={open} />
          )}
        </section>
      </main>
    </div>
  );
}
