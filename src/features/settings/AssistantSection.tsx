// Assistant settings: where `claude` lives and which folder its sessions start in.
import { useEffect, useState } from "react";
import type { AssistantSettings, Settings } from "../../lib/settings";
import "./appearance.css";

interface Props {
  settings: Settings;
  onChange: (next: Settings) => void;
}

/** A path typed freely and saved on blur or Enter; Rust stores only absolute paths. */
function PathField({
  id,
  value,
  placeholder,
  onSave,
}: {
  id: string;
  value: string | null;
  placeholder: string;
  onSave: (value: string | null) => void;
}) {
  const [draft, setDraft] = useState(value ?? "");
  useEffect(() => setDraft(value ?? ""), [value]);
  const trimmed = draft.trim();
  const invalid = trimmed !== "" && !/^(\/|[A-Za-z]:[\\/])/.test(trimmed);
  const save = () => {
    if (!invalid && (trimmed || null) !== value) onSave(trimmed || null);
  };

  return (
    <div className="assistant-path">
      <input
        aria-labelledby={id}
        value={draft}
        placeholder={placeholder}
        spellCheck={false}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={save}
        onKeyDown={(event) => event.key === "Enter" && save()}
      />
      {invalid && <span className="assistant-path-error">Use an absolute path.</span>}
    </div>
  );
}

export function AssistantSection({ settings, onChange }: Props) {
  const assistant = settings.assistant;
  const update = (patch: Partial<AssistantSettings>) => onChange({ ...settings, assistant: { ...assistant, ...patch } });

  return (
    <div className="appearance">
      <section className="appearance-row">
        <div className="appearance-text">
          <h3 className="appearance-label">Assistant</h3>
          <p className="appearance-hint">
            The assistant panel ({"⌘"}J or Ctrl+J) runs Claude Code in a terminal, signed in with your own Claude
            account or API key. ADO Desk never sees those credentials. Install Claude Code from code.claude.com.
          </p>
        </div>
      </section>

      <section className="appearance-row">
        <div className="appearance-text">
          <h3 className="appearance-label" id="assistant-claude-path">
            Claude Code path
          </h3>
          <p className="appearance-hint">Leave empty to find it on your shell PATH or in ~/.local/bin.</p>
        </div>
        <PathField
          id="assistant-claude-path"
          value={assistant.claudePath}
          placeholder="/Users/you/.local/bin/claude"
          onSave={(claudePath) => update({ claudePath })}
        />
      </section>

      <section className="appearance-row">
        <div className="appearance-text">
          <h3 className="appearance-label" id="assistant-folder">
            Working folder
          </h3>
          <p className="appearance-hint">
            Where sessions start; Claude Code can read and change files there. Leave empty for an empty folder owned by
            ADO Desk. Takes effect on the next session.
          </p>
        </div>
        <PathField
          id="assistant-folder"
          value={assistant.workingFolder}
          placeholder="Empty app folder"
          onSave={(workingFolder) => update({ workingFolder })}
        />
      </section>
    </div>
  );
}
