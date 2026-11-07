// Quick search (Cmd+K). Requests run in Rust (src-tauri/src/search.rs).
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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

/** Rust -> bar window: the bar was just shown (start with an empty query). */
export const SEARCH_BAR_SHOWN_EVENT = "search-bar-shown";
/** Rust -> main window: open this item (unvalidated JSON from the bar). */
export const SEARCH_BAR_OPEN_EVENT = "search-bar-open";

export const searchApi = {
  /** `#123` work item, `!123` pull request, `123` either, other text searches titles. */
  quickSearch: (query: string) => call<SearchResults>("quick_search", { query }),
  /** Bar window: hide it (focus returns to the previous app when nothing else is visible). */
  hideBar: () => call<void>("hide_search_bar"),
  /** Bar window: hide it and open `hit` in the main window. */
  openFromBar: (hit: SearchHit) => call<void>("open_search_hit", { hit }),
  onBarShown: (handler: () => void): Promise<UnlistenFn> => listen(SEARCH_BAR_SHOWN_EVENT, () => handler()),
  onOpenFromBar: (handler: (hit: unknown) => void): Promise<UnlistenFn> =>
    listen<unknown>(SEARCH_BAR_OPEN_EVENT, (event) => handler(event.payload)),
};

export function hitKey(hit: SearchHit): string {
  return `${hit.organization}/${hit.kind}/${hit.item.id}`;
}
