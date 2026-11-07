// Shortcuts settings: the global shortcut that opens the floating search bar. Rust validates
// and registers it; an invalid or taken combination comes back as a save error.
import { useState, type KeyboardEvent } from "react";
import type { SearchSettings, Settings } from "../../lib/settings";
import "./appearance.css";

const MAC = navigator.userAgent.includes("Mac");
const MODIFIER_KEYS = new Set(["Meta", "Control", "Alt", "Shift", "OS"]);

/** Accelerator for a key press, or null while only modifiers are held. */
export function acceleratorOf(event: Pick<KeyboardEvent, "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">): string | null {
  if (MODIFIER_KEYS.has(event.key) || !event.code) return null;
  const parts: string[] = [];
  if (event.metaKey) parts.push(MAC ? "Command" : "Super");
  if (event.ctrlKey) parts.push("Control");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  parts.push(event.code);
  return parts.join("+");
}

/** True when the accelerator has a modifier other than Shift (Rust enforces the same rule). */
function hasCommandModifier(accelerator: string): boolean {
  return /(^|\+)(Command|Cmd|Super|Control|Ctrl|Alt|Option|CommandOrControl)\+/i.test(accelerator);
}

const SYMBOLS: Record<string, [mac: string, other: string]> = {
  commandorcontrol: ["⌘", "Ctrl"],
  command: ["⌘", "Win"],
  cmd: ["⌘", "Win"],
  super: ["⌘", "Win"],
  control: ["⌃", "Ctrl"],
  ctrl: ["⌃", "Ctrl"],
  alt: ["⌥", "Alt"],
  option: ["⌥", "Alt"],
  shift: ["⇧", "Shift"],
};

/** Human label: `CommandOrControl+Shift+KeyK` -> `⌘⇧K` on macOS, `Ctrl+Shift+K` elsewhere. */
export function shortcutLabel(accelerator: string): string {
  const parts = accelerator.split("+").map((token) => {
    const symbol = SYMBOLS[token.toLowerCase()];
    if (symbol) return MAC ? symbol[0] : symbol[1];
    return token.replace(/^Key(?=[A-Z]$)/, "").replace(/^Digit(?=\d$)/, "");
  });
  return parts.join(MAC ? "" : "+");
}

interface Props {
  settings: Settings;
  onChange: (next: Settings) => void;
}

export function ShortcutsSection({ settings, onChange }: Props) {
  const search = settings.search;
  const [recording, setRecording] = useState(false);
  const [hint, setHint] = useState<string | null>(null);
  const update = (patch: Partial<SearchSettings>) => onChange({ ...settings, search: { ...search, ...patch } });

  const record = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape" && !event.metaKey && !event.ctrlKey && !event.altKey) {
      setRecording(false);
      setHint(null);
      return;
    }
    const accelerator = acceleratorOf(event);
    if (!accelerator) return;
    if (!hasCommandModifier(accelerator)) {
      setHint(`Add ${MAC ? "⌘, ⌃ or ⌥" : "Ctrl, Alt or Win"} so the shortcut does not catch normal typing.`);
      return;
    }
    setRecording(false);
    setHint(null);
    if (accelerator !== search.globalShortcut) update({ globalShortcut: accelerator });
  };

  return (
    <div className="appearance">
      <section className="appearance-row">
        <div className="appearance-text">
          <h3 className="appearance-label" id="shortcut-enabled">
            Global search shortcut
          </h3>
          <p className="appearance-hint">
            Opens the search bar from any app, even when ADO Desk is in the background. It does not appear over
            full-screen apps.
          </p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={search.globalShortcutEnabled}
          aria-labelledby="shortcut-enabled"
          className={`appearance-switch${search.globalShortcutEnabled ? " is-on" : ""}`}
          onClick={() => update({ globalShortcutEnabled: !search.globalShortcutEnabled })}
        >
          <span className="appearance-switch-thumb" />
        </button>
      </section>

      <section className={`appearance-row${search.globalShortcutEnabled ? "" : " is-disabled"}`}>
        <div className="appearance-text">
          <h3 className="appearance-label" id="shortcut-keys">
            Shortcut
          </h3>
          <p className="appearance-hint" aria-live="polite">
            {recording
              ? hint ?? "Press the new combination, or Esc to cancel."
              : "Click the shortcut to record a new one."}
          </p>
        </div>
        <button
          type="button"
          className={`btn btn-sm shortcut-recorder${recording ? " is-recording" : ""}`}
          aria-labelledby="shortcut-keys"
          disabled={!search.globalShortcutEnabled}
          onClick={() => {
            setRecording(true);
            setHint(null);
          }}
          onKeyDown={record}
          onBlur={() => setRecording(false)}
        >
          {recording ? "Recording…" : <kbd>{shortcutLabel(search.globalShortcut)}</kbd>}
        </button>
      </section>
    </div>
  );
}
