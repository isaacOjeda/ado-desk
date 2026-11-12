// In-window signal from a detail view to the Workspace: "open the assistant with this prompt".
// Prompts name the item by id only: titles are written by other people and are not typed into
// the user's own prompt (Claude reads them through the ado-desk tools instead).

type Listener = (prompt: string) => void;

const listeners = new Set<Listener>();

export function askAssistant(prompt: string): void {
  listeners.forEach((listener) => listener(prompt));
}

/** Returns the unsubscribe function (usable directly as a React effect cleanup). */
export function onAskAssistant(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export const workItemPrompt = (id: number) => `About work item ${id}, open in ADO Desk: `;
export const pullRequestPrompt = (id: number) => `About pull request ${id}, open in ADO Desk: `;
