// Quick search (Cmd+K). Requests run in Rust (src-tauri/src/search.rs).
import { call, type WorkItem } from "../../lib/api";
import type { FilteredPullRequest } from "../filters/api";

/** Mirrors `search::SearchHit`: items have the same shape as the lists. */
export type SearchHit =
  | { kind: "workItem"; organization: string; item: WorkItem }
  | { kind: "pullRequest"; organization: string; item: FilteredPullRequest };

/** Mirrors `search::SearchResults`. */
export interface SearchResults {
  hits: SearchHit[];
  /** Organizations that could not be searched (for example an expired token). */
  unavailable: string[];
}

export const searchApi = {
  /** `#123` work item, `!123` pull request, `123` either, other text searches titles. */
  quickSearch: (query: string) => call<SearchResults>("quick_search", { query }),
};

export function hitKey(hit: SearchHit): string {
  return `${hit.organization}/${hit.kind}/${hit.item.id}`;
}
