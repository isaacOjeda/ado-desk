// Presentational filter controls: no data fetching, no storage.
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import "./filters.css";

export interface Option {
  value: string;
  label: string;
}

export function FilterBar({
  activeCount,
  onReset,
  onSaveView,
  children,
}: {
  activeCount: number;
  onReset: () => void;
  /** Saves the current filters as a named view; rejects with a message to show. */
  onSaveView?: (name: string) => Promise<void>;
  children: ReactNode;
}) {
  return (
    <div className="filter-bar" role="toolbar" aria-label="Filters">
      {children}
      <span className="filter-bar-end">
        {activeCount > 0 && (
          <span className="filter-count" aria-live="polite">
            {activeCount} {activeCount === 1 ? "filter" : "filters"}
          </span>
        )}
        <button className="btn btn-ghost btn-sm" onClick={onReset} disabled={activeCount === 0}>
          Reset
        </button>
        {onSaveView && <SaveView onSave={onSaveView} />}
      </span>
    </div>
  );
}

/** "Save view" button that turns into an inline name field: Enter saves, Esc cancels. */
function SaveView({ onSave }: { onSave: (name: string) => Promise<void> }) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const close = () => {
    setEditing(false);
    setName("");
    setError(null);
  };

  const submit = () => {
    if (saving) return;
    if (name.trim() === "") {
      setError("Name the view first.");
      return;
    }
    setSaving(true);
    setError(null);
    onSave(name.trim())
      .then(close)
      .catch((err: unknown) => setError(err instanceof Error ? err.message : "Could not save the view."))
      .finally(() => setSaving(false));
  };

  if (!editing) {
    return (
      <button className="btn btn-ghost btn-sm" onClick={() => setEditing(true)} title="Save these filters as a view">
        Save view
      </button>
    );
  }

  return (
    <span className="filter-save">
      <input
        className="filter-save-input"
        autoFocus
        placeholder="View name"
        aria-label="View name"
        maxLength={60}
        value={name}
        disabled={saving}
        onChange={(e) => {
          setName(e.target.value);
          setError(null);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") submit();
          if (e.key === "Escape") {
            e.stopPropagation();
            close();
          }
        }}
      />
      <button className="btn btn-sm" onClick={submit} disabled={saving}>
        Save
      </button>
      <button className="btn btn-ghost btn-sm" onClick={close} disabled={saving}>
        Cancel
      </button>
      {error && (
        <span className="filter-save-error" role="alert">
          {error}
        </span>
      )}
    </span>
  );
}

/** A labelled native select; highlighted when it differs from its default. */
export function FilterSelect({
  label,
  value,
  options,
  onChange,
  isDefault,
  disabled = false,
  title,
}: {
  label: string;
  value: string;
  options: Option[];
  onChange: (value: string) => void;
  isDefault: boolean;
  disabled?: boolean;
  title?: string;
}) {
  const id = useId();
  return (
    <span className={`filter-field ${isDefault ? "" : "is-set"} ${disabled ? "is-disabled" : ""}`} title={title}>
      <label htmlFor={id}>{label}</label>
      <select id={id} value={value} disabled={disabled} onChange={(e) => onChange(e.target.value)}>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </span>
  );
}

/** A dropdown of checkboxes; an empty selection means "All". */
export function MultiSelect({
  label,
  options,
  selected,
  onChange,
  loading = false,
}: {
  label: string;
  options: string[];
  selected: string[];
  onChange: (selected: string[]) => void;
  loading?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setOpen(false);
      }
    };
    document.addEventListener("pointerdown", onPointer);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [open]);

  // Keep selected values visible even when the current project does not define them.
  const all = [...options, ...selected.filter((s) => !options.includes(s))];
  const summary = selected.length === 0 ? "All" : selected.length === 1 ? selected[0] : `${selected.length} selected`;
  const toggle = (value: string) =>
    onChange(selected.includes(value) ? selected.filter((s) => s !== value) : [...selected, value]);

  return (
    <span ref={root} className={`filter-field filter-multi ${selected.length ? "is-set" : ""}`}>
      <span className="filter-label">{label}</span>
      <button className="filter-trigger" aria-haspopup="true" aria-expanded={open} onClick={() => setOpen(!open)}>
        {summary}
        <span aria-hidden className="filter-caret">
          ▾
        </span>
      </button>
      {open && (
        <div className="filter-popover" role="group" aria-label={label}>
          {loading && all.length === 0 && <p className="filter-empty">Loading…</p>}
          {!loading && all.length === 0 && <p className="filter-empty">No types</p>}
          {all.map((value) => (
            <label key={value} className="filter-option">
              <input type="checkbox" checked={selected.includes(value)} onChange={() => toggle(value)} />
              <span>{value}</span>
            </label>
          ))}
          {selected.length > 0 && (
            <button className="btn btn-ghost btn-sm filter-clear" onClick={() => onChange([])}>
              Clear
            </button>
          )}
        </div>
      )}
    </span>
  );
}
