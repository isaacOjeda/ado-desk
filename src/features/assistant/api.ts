// Assistant panel: the user's own Claude Code in a terminal run by Rust (assistant/terminal.rs).
import { Channel } from "@tauri-apps/api/core";
import { call } from "../../lib/api";

/** Mirrors `assistant::terminal::TerminalEvent`. */
export type TerminalEvent = { kind: "output"; data: string } | { kind: "exit"; code: number | null };

export const assistantApi = {
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
