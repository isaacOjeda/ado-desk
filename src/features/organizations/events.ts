// In-window signal from the sidebar switcher to App: "show the add-organization screen".
// The Sidebar is rendered by the Workspace, so it cannot reach App through props.

type Listener = () => void;

const listeners = new Set<Listener>();

export function requestAddOrganization(): void {
  listeners.forEach((listener) => listener());
}

/** Returns the unsubscribe function (usable directly as a React effect cleanup). */
export function onAddOrganizationRequested(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
