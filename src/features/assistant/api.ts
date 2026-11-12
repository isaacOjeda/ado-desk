// Assistant panel: the user's own Claude Code in a terminal run by Rust (assistant/terminal.rs).
import { Channel } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call } from "../../lib/api";

/** Mirrors `assistant::terminal::TerminalEvent`. */
export type TerminalEvent = { kind: "output"; data: string } | { kind: "exit"; code: number | null };

/** Mirrors `assistant::context::ViewContext`: what the main window shows, for `current_view`. */
export type ViewContext =
  | { kind: "workItem"; id: number; project: string; title: string }
  | { kind: "pullRequest"; id: number; project: string; repositoryId: string; repository: string; title: string }
  | { kind: "list"; list: "inbox" | "workItems" | "pullRequests" };

/** Mirrors `assistant::confirm::WriteRequest`: a write the session asked for, awaiting the user. */
export interface WriteRequest {
  id: number;
  action: string;
  target: string;
  organization: string;
  /** The exact text that will be posted. */
  text: string;
}

export const assistantApi = {
  /** Approves or declines a write request. */
  confirm: (id: number, approved: boolean) => call<void>("assistant_confirm", { id, approved }),
  onConfirmRequested: (handler: (request: WriteRequest) => void): Promise<UnlistenFn> =>
    listen<WriteRequest>("assistant-confirm", (event) => handler(event.payload)),
  /** A request answered or expired (also from another path): its dialog closes. */
  onConfirmClosed: (handler: (id: number) => void): Promise<UnlistenFn> =>
    listen<number>("assistant-confirm-closed", (event) => handler(event.payload)),
  /** Records what the main window shows; Rust tags it with the active organization. */
  setView: (view: ViewContext | null) => call<void>("assistant_set_view", { view }),
  /** Starts `claude` in a new terminal (ending any previous one); resolves with the session id. */
  start: (cols: number, rows: number, onEvent: (event: TerminalEvent) => void) => {
    const events = new Channel<TerminalEvent>();
    events.onmessage = onEvent;
    return call<number>("assistant_start", { cols, rows, events });
  },
  write: (id: number, data: string) => call<void>("assistant_write", { id, data }),
  resize: (id: number, cols: number, rows: number) => call<void>("assistant_resize", { id, cols, rows }),
  stop: (id: number) => call<void>("assistant_stop", { id }),
};
