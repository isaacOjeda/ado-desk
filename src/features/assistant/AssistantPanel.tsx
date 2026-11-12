// Bottom panel running the user's own Claude Code (signed in with their own account) in a
// terminal. The panel stays mounted while hidden, so closing it does not end the session.
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { Terminal, type ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { ApiError } from "../../lib/api";
import { MOD_KEY } from "../../lib/platform";
import { assistantApi } from "./api";
import "./assistant.css";

type Status =
  | { kind: "starting" }
  | { kind: "running" }
  | { kind: "exited"; code: number | null }
  | { kind: "failed"; message: string; notFound: boolean };

const HEIGHT_KEY = "ado-desk.assistant.height";
const MIN_HEIGHT = 140;
const DEFAULT_HEIGHT = 300;

function savedHeight(): number {
  try {
    const value = Number(window.localStorage.getItem(HEIGHT_KEY));
    return Number.isFinite(value) && value >= MIN_HEIGHT ? value : DEFAULT_HEIGHT;
  } catch {
    return DEFAULT_HEIGHT;
  }
}

function saveHeight(height: number) {
  try {
    window.localStorage.setItem(HEIGHT_KEY, String(Math.round(height)));
  } catch {
    // A per-viewer convenience: the default height is fine without storage.
  }
}

/** Terminal colors from the active theme's CSS variables. */
function themeColors(): ITheme {
  const css = getComputedStyle(document.documentElement);
  const value = (name: string) => css.getPropertyValue(name).trim() || undefined;
  return {
    background: value("--content-bg"),
    foreground: value("--text"),
    cursor: value("--accent"),
    cursorAccent: value("--content-bg"),
    selectionBackground: value("--surface-hover"),
  };
}

interface Props {
  open: boolean;
  onClose: () => void;
  onOpenSettings: () => void;
}

export function AssistantPanel({ open, onClose, onOpenSettings }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const [status, setStatus] = useState<Status>({ kind: "starting" });
  const [run, setRun] = useState(0);
  const [height, setHeight] = useState(savedHeight);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const terminal = new Terminal({
      fontFamily: '"SF Mono", ui-monospace, Menlo, Consolas, monospace',
      fontSize: 12,
      cursorBlink: true,
      theme: themeColors(),
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(host);
    fit.fit();
    terminalRef.current = terminal;

    let id: number | null = null;
    let disposed = false;
    setStatus({ kind: "starting" });

    const input = terminal.onData((data) => {
      if (id !== null) assistantApi.write(id, data).catch(() => {});
    });
    const resize = terminal.onResize(({ cols, rows }) => {
      if (id !== null) assistantApi.resize(id, cols, rows).catch(() => {});
    });
    // Deferred a tick: a mount that is torn down at once (React StrictMode in development)
    // never starts a process.
    const timer = window.setTimeout(() => {
    // The size the session started with: a resize before the id arrives is otherwise lost.
    const started = { cols: terminal.cols, rows: terminal.rows };
    assistantApi
      .start(started.cols, started.rows, (event) => {
        if (disposed) return;
        if (event.kind === "output") terminal.write(event.data);
        else setStatus({ kind: "exited", code: event.code });
      })
      .then((sessionId) => {
        if (disposed) {
          assistantApi.stop(sessionId).catch(() => {});
          return;
        }
        id = sessionId;
        // A host that was hidden (after a failure) or resized during startup measures now.
        fit.fit();
        if (terminal.cols !== started.cols || terminal.rows !== started.rows) {
          assistantApi.resize(sessionId, terminal.cols, terminal.rows).catch(() => {});
        }
        setStatus((current) => (current.kind === "starting" ? { kind: "running" } : current));
        terminal.focus();
      })
      .catch((err: unknown) => {
        if (disposed) return;
        const message = err instanceof Error ? err.message : String(err);
        setStatus({ kind: "failed", message, notFound: err instanceof ApiError && err.kind === "assistantNotFound" });
      });
    }, 0);

    // A hidden panel measures 0x0, which fit ignores; it refits when shown again.
    const observer = new ResizeObserver(() => fit.fit());
    observer.observe(host);
    // Theme changes rewrite the CSS variables on <html>.
    const themeWatcher = new MutationObserver(() => {
      terminal.options.theme = themeColors();
    });
    themeWatcher.observe(document.documentElement, { attributes: true, attributeFilter: ["style", "data-theme"] });

    return () => {
      disposed = true;
      window.clearTimeout(timer);
      observer.disconnect();
      themeWatcher.disconnect();
      input.dispose();
      resize.dispose();
      if (id !== null) assistantApi.stop(id).catch(() => {});
      terminalRef.current = null;
      terminal.dispose();
    };
  }, [run]);

  useEffect(() => {
    if (open) terminalRef.current?.focus();
  }, [open]);

  const startResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const startY = event.clientY;
    const startHeight = height;
    const max = () => Math.max(MIN_HEIGHT, window.innerHeight * 0.8);
    let next = startHeight;
    const move = (e: PointerEvent) => {
      next = Math.min(max(), Math.max(MIN_HEIGHT, startHeight + startY - e.clientY));
      setHeight(next);
    };
    const end = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      saveHeight(next);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
  };

  return (
    <section className="assistant-panel" hidden={!open} style={{ height }} aria-label="Assistant">
      <div
        className="assistant-resize"
        role="separator"
        aria-orientation="horizontal"
        aria-label="Resize the assistant panel"
        onPointerDown={startResize}
      />
      <header className="assistant-header">
        <span className="assistant-title">Assistant</span>
        <span className="assistant-subtitle">Claude Code, signed in with your own account</span>
        <span className="assistant-spacer" />
        <button className="btn btn-ghost btn-sm" onClick={() => setRun((n) => n + 1)} title="End this session and start a new one">
          Restart
        </button>
        <button className="btn btn-ghost btn-sm" onClick={onClose} title={`Hide (${MOD_KEY}J)`} aria-label="Hide the assistant">
          ✕
        </button>
      </header>
      {status.kind === "failed" ? (
        <div className="assistant-message" role="alert">
          <p>{status.message}</p>
          <div className="assistant-message-actions">
            {status.notFound && (
              <button className="btn btn-sm" onClick={onOpenSettings}>
                Open Settings
              </button>
            )}
            <button className="btn btn-primary btn-sm" onClick={() => setRun((n) => n + 1)}>
              Try again
            </button>
          </div>
        </div>
      ) : status.kind === "exited" ? (
        <div className="assistant-banner">
          <span>Claude Code exited{status.code !== null && status.code !== 0 ? ` (code ${status.code})` : ""}.</span>
          <button className="btn btn-primary btn-sm" onClick={() => setRun((n) => n + 1)}>
            Start again
          </button>
        </div>
      ) : null}
      <div className="assistant-terminal" ref={hostRef} hidden={status.kind === "failed"} />
    </section>
  );
}
