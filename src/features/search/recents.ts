// Recently opened items, shown by the quick search before anything is typed. Stored per
// device in localStorage and validated on read, since the detail views render them directly.
import type { SearchHit } from "./api";
import { hitKey } from "./api";

const KEY = "ado-desk.recent-items";
export const MAX_RECENTS = 10;

const PR_STATUSES = new Set(["active", "completed", "abandoned"]);

type Shape = Record<string, unknown>;

function strings(value: Shape, keys: string[]): boolean {
  return keys.every((key) => typeof value[key] === "string");
}

/** True when `value` has every field the workspace needs to open it. */
export function isHit(value: unknown): value is SearchHit {
  if (typeof value !== "object" || value === null) return false;
  const hit = value as Shape;
  const item = hit.item as Shape | null;
  if (typeof hit.organization !== "string" || !hit.organization) return false;
  if (typeof item !== "object" || item === null) return false;
  if (!Number.isInteger(item.id) || (item.id as number) <= 0) return false;
  if (!strings(item, ["title", "project", "url"])) return false;
  // Cmd+Enter hands the URL to the system opener: only HTTPS links.
  if (!(item.url as string).startsWith("https://")) return false;
  if (hit.kind === "workItem") return strings(item, ["workItemType", "state"]);
  if (hit.kind === "pullRequest") {
    return (
      strings(item, ["repository", "repositoryId", "sourceBranch", "targetBranch", "author", "createdAt"]) &&
      PR_STATUSES.has(item.status as string)
    );
  }
  return false;
}

export function readRecents(): SearchHit[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter(isHit).slice(0, MAX_RECENTS) : [];
  } catch {
    return [];
  }
}

/** Moves `hit` to the top of the recent items. */
export function recordRecent(hit: SearchHit): void {
  const key = hitKey(hit);
  const next = [hit, ...readRecents().filter((recent) => hitKey(recent) !== key)].slice(0, MAX_RECENTS);
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    // Storage full or unavailable: recents are a convenience.
  }
}
