// Cmd/Ctrl+K palette: open a work item or pull request by ID, or find one by title.
// Searches run in Rust; the palette only debounces input and keeps the latest response.
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ApiError } from "../../lib/api";
import { pullRequestStatusBadge, stateTone, workItemTypeTone } from "../../lib/format";
import { Badge, Dot, Spinner } from "../../components/ui";
import { hitKey, searchApi, type SearchHit, type SearchResults } from "./api";
import { readRecents } from "./recents";
import "./search.css";

const SEARCH_DELAY_MS = 350;
const MAX_QUERY_LENGTH = 100;
export const MOD_KEY = navigator.userAgent.includes("Mac") ? "⌘" : "Ctrl+";

interface Props {
  /** The active organization; results from other organizations show their name. */
  organization: string;
  /** Rejects when the item cannot be opened (for example its organization cannot connect). */
  onOpen: (hit: SearchHit) => Promise<void>;
  onClose: () => void;
}

interface Section {
  title: string;
  hits: SearchHit[];
}

function errorText(error: unknown): string {
  return error instanceof ApiError ? error.message : "Search failed. Try again.";
}

function sectionsOf(hits: SearchHit[]): Section[] {
  const workItems = hits.filter((hit) => hit.kind === "workItem");
  const pullRequests = hits.filter((hit) => hit.kind === "pullRequest");
  return [
    { title: "Work items", hits: workItems },
    { title: "Pull requests", hits: pullRequests },
  ].filter((section) => section.hits.length > 0);
}

export function QuickSearch({ organization, onOpen, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResults | null>(null);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const [opening, setOpening] = useState<string | null>(null);
  const request = useRef(0);
  const list = useRef<HTMLUListElement>(null);
  const recents = useMemo(() => readRecents(), []);
  const typed = query.trim() !== "";

  useEffect(() => {
    // Only the latest search may update the results; older responses are dropped.
    const id = ++request.current;
    if (!typed) {
      setResults(null);
      setError(null);
      setSearching(false);
      return;
    }
    const timer = window.setTimeout(() => {
      setSearching(true);
      searchApi
        .quickSearch(query)
        .then((found) => {
          if (request.current !== id) return;
          setResults(found);
          setError(null);
          setActive(0);
        })
        .catch((err) => {
          if (request.current !== id) return;
          setResults(null);
          setError(errorText(err));
        })
        .finally(() => request.current === id && setSearching(false));
    }, SEARCH_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [query, typed]);

  const sections: Section[] = typed
    ? sectionsOf(results?.hits ?? [])
    : recents.length > 0
      ? [{ title: "Recent", hits: recents }]
      : [];
  const flat = sections.flatMap((section) => section.hits);
  const current = flat[Math.min(active, flat.length - 1)];

  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }, [active, results]);

  const open = (hit: SearchHit) => {
    if (opening) return;
    setOpening(hit.organization === organization ? null : hit.organization);
    setError(null);
    onOpen(hit).catch((err) => {
      setOpening(null);
      setError(errorText(err));
    });
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    // The palette owns the keyboard while open (Esc must not also close a detail view).
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (flat.length === 0) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((index) => (Math.min(index, flat.length - 1) + step + flat.length) % flat.length);
    } else if (event.key === "Enter" && current) {
      event.preventDefault();
      if (event.metaKey || event.ctrlKey) {
        void openUrl(current.item.url);
        onClose();
      } else {
        open(current);
      }
    }
  };

  const unavailable = results?.unavailable ?? [];
  let status: string | null = null;
  if (opening) status = `Switching to ${opening}…`;
  else if (searching && !results) status = "Searching…";
  else if (typed && results && flat.length === 0) status = "No work items or pull requests match.";
  else if (!typed && recents.length === 0)
    status = "Type #123 for a work item, !45 for a pull request, or words from a title.";

  return (
    <div className="qs-backdrop" onMouseDown={onClose}>
      <div
        className="qs"
        role="dialog"
        aria-modal="true"
        aria-label="Quick search"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <div className="qs-input-row">
          <span className="qs-icon" aria-hidden>
            ⌕
          </span>
          <input
            className="qs-input"
            placeholder="Search work items and pull requests"
            aria-label="Search work items and pull requests"
            role="combobox"
            aria-expanded={flat.length > 0}
            aria-controls="qs-results"
            aria-activedescendant={current ? `qs-${hitKey(current)}` : undefined}
            value={query}
            maxLength={MAX_QUERY_LENGTH}
            autoFocus
            spellCheck={false}
            onChange={(event) => setQuery(event.target.value)}
          />
          {searching && <Spinner />}
        </div>

        {error && (
          <p className="qs-notice is-error" role="alert">
            {error}
          </p>
        )}
        {unavailable.length > 0 && (
          <p className="qs-notice">Could not search {unavailable.join(", ")} right now.</p>
        )}

        <ul className="qs-results" id="qs-results" role="listbox" ref={list} aria-label="Results">
          {sections.map((section) => (
            <li key={section.title} role="presentation">
              <div className="qs-section">{section.title}</div>
              <ul role="presentation">
                {section.hits.map((hit) => (
                  <HitRow
                    key={hitKey(hit)}
                    hit={hit}
                    showOrganization={hit.organization !== organization}
                    selected={current === hit}
                    onHover={() => setActive(flat.indexOf(hit))}
                    onOpen={() => open(hit)}
                  />
                ))}
              </ul>
            </li>
          ))}
        </ul>
        {status && <p className="qs-status">{status}</p>}

        <footer className="qs-footer">
          <span>
            <kbd>↑</kbd>
            <kbd>↓</kbd> move
          </span>
          <span>
            <kbd>↵</kbd> open
          </span>
          <span>
            <kbd>{MOD_KEY}↵</kbd> open in Azure DevOps
          </span>
          <span>
            <kbd>esc</kbd> close
          </span>
        </footer>
      </div>
    </div>
  );
}

interface HitRowProps {
  hit: SearchHit;
  showOrganization: boolean;
  selected: boolean;
  onHover: () => void;
  onOpen: () => void;
}

function HitRow({ hit, showOrganization, selected, onHover, onOpen }: HitRowProps) {
  const organization = showOrganization ? ` · ${hit.organization}` : "";
  return (
    <li
      id={`qs-${hitKey(hit)}`}
      role="option"
      aria-selected={selected}
      className={`qs-row ${selected ? "is-active" : ""}`}
      onMouseMove={selected ? undefined : onHover}
      // Keeps focus in the input, so the keyboard (Esc, arrows) still drives the palette.
      onMouseDown={(event) => event.preventDefault()}
      onClick={onOpen}
    >
      {hit.kind === "workItem" ? (
        <>
          <Dot tone={workItemTypeTone(hit.item.workItemType)} />
          <span className="qs-id">#{hit.item.id}</span>
          <span className="qs-main">
            <span className="qs-title">{hit.item.title}</span>
            <span className="qs-meta">
              {hit.item.workItemType} · {hit.item.project}
              {organization}
            </span>
          </span>
          <Badge tone={stateTone(hit.item.state)}>{hit.item.state}</Badge>
        </>
      ) : (
        <PullRequestHit hit={hit} organization={organization} />
      )}
    </li>
  );
}

function PullRequestHit({ hit, organization }: { hit: Extract<SearchHit, { kind: "pullRequest" }>; organization: string }) {
  const pr = hit.item;
  const closed = pullRequestStatusBadge(pr.status);
  return (
    <>
      <span className="qs-pr-icon" aria-hidden>
        ⇄
      </span>
      <span className="qs-id">!{pr.id}</span>
      <span className="qs-main">
        <span className="qs-title">{pr.title}</span>
        <span className="qs-meta">
          {pr.repository} · {pr.project}
          {organization}
        </span>
      </span>
      {closed ? <Badge tone={closed.tone}>{closed.label}</Badge> : pr.isDraft ? <Badge>Draft</Badge> : null}
    </>
  );
}
