// Inbox filters: client-side, over the items the background checks already found.
// The Inbox spans every organization, so its filters are remembered once per device.
import { useCallback, useState } from "react";
import type { InboxItem, InboxKind } from "./api";

/** A project within one organization: the Inbox mixes organizations that may share project names. */
export interface InboxProject {
  organization: string;
  project: string;
}

export interface InboxFilter {
  kind: InboxKind | "all";
  state: "all" | "unread";
  project: InboxProject | null;
}

export const DEFAULT_INBOX_FILTER: InboxFilter = { kind: "all", state: "all", project: null };

const KINDS: readonly string[] = ["reviewRequest", "assignment", "comment"];
const STORAGE_KEY = "ado-desk.filters.inbox";

/** Validates an untrusted stored value; unknown fields fall back to the defaults. */
export function parseInboxFilter(raw: unknown): InboxFilter {
  if (typeof raw !== "object" || raw === null) return DEFAULT_INBOX_FILTER;
  const { kind, state, project } = raw as Record<string, unknown>;
  return {
    kind: typeof kind === "string" && KINDS.includes(kind) ? (kind as InboxKind) : "all",
    state: state === "unread" ? "unread" : "all",
    project: parseProject(project),
  };
}

function parseProject(raw: unknown): InboxProject | null {
  if (typeof raw !== "object" || raw === null) return null;
  const { organization, project } = raw as Record<string, unknown>;
  return typeof organization === "string" && organization !== "" && typeof project === "string" && project !== ""
    ? { organization, project }
    : null;
}

export function itemProject(item: InboxItem): InboxProject {
  return { organization: item.target.organization, project: item.target.item.project };
}

export function sameProject(a: InboxProject, b: InboxProject): boolean {
  return a.organization.toLowerCase() === b.organization.toLowerCase() && a.project === b.project;
}

/** Distinct projects of the Inbox items, sorted by organization then project. */
export function inboxProjects(items: InboxItem[]): InboxProject[] {
  const projects: InboxProject[] = [];
  for (const item of items) {
    const p = itemProject(item);
    if (!projects.some((q) => sameProject(p, q))) projects.push(p);
  }
  return projects.sort((a, b) => a.organization.localeCompare(b.organization) || a.project.localeCompare(b.project));
}

export function countInboxFilters(filter: InboxFilter): number {
  return [filter.kind !== "all", filter.state !== "all", filter.project !== null].filter(Boolean).length;
}

export function matchesInboxFilter(item: InboxItem, filter: InboxFilter): boolean {
  return (
    (filter.kind === "all" || item.kind === filter.kind) &&
    (filter.state === "all" || !item.read) &&
    (filter.project === null || sameProject(itemProject(item), filter.project))
  );
}

function load(): InboxFilter {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return parseInboxFilter(stored === null ? undefined : JSON.parse(stored));
  } catch {
    return DEFAULT_INBOX_FILTER;
  }
}

/** Inbox filter state remembered in `localStorage` (best effort). */
export function useInboxFilter(): [InboxFilter, (next: InboxFilter) => void] {
  const [filter, setFilter] = useState(load);
  const update = useCallback((next: InboxFilter) => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    } catch {
      // Unavailable storage: the filter still applies for this session.
    }
    setFilter(next);
  }, []);
  return [filter, update];
}
