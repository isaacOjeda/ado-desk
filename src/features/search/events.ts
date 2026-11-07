// In-window signals to App: "open the quick search" (sidebar button, Cmd/Ctrl+K) and "open
// this item" (Inbox), since App renders the palette and owns organization switches.
import type { SearchHit } from "./api";

type Listener = () => void;

const listeners = new Set<Listener>();

export function requestQuickSearch(): void {
  listeners.forEach((listener) => listener());
}

type OpenListener = (hit: SearchHit) => void;

const openListeners = new Set<OpenListener>();

/** Opens an item (switching organization when needed); App owns the switch. */
export function requestOpen(hit: SearchHit): void {
  openListeners.forEach((listener) => listener(hit));
}

export function onOpenRequested(listener: OpenListener): () => void {
  openListeners.add(listener);
  return () => {
    openListeners.delete(listener);
  };
}

/** Returns the unsubscribe function (usable directly as a React effect cleanup). */
export function onQuickSearchRequested(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
