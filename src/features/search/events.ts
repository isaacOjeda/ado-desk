// In-window signal "open the quick search": the sidebar button and the Cmd/Ctrl+K handler
// both reach App, which renders the palette above the workspace.

type Listener = () => void;

const listeners = new Set<Listener>();

export function requestQuickSearch(): void {
  listeners.forEach((listener) => listener());
}

/** Returns the unsubscribe function (usable directly as a React effect cleanup). */
export function onQuickSearchRequested(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
