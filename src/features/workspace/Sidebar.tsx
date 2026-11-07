import type { Session } from "../../lib/api";
import { initials } from "../../lib/format";

export type View = "workItems" | "pullRequests";

interface Props {
  session: Session;
  view: View;
  counts: Record<View, number | null>;
  onSelect: (view: View) => void;
  onSignOut: () => void;
  onOpenSettings: () => void;
}

const NAV: { view: View; label: string; icon: string }[] = [
  { view: "workItems", label: "My work items", icon: "▤" },
  { view: "pullRequests", label: "Pull requests", icon: "⇄" },
];

export function Sidebar({ session, view, counts, onSelect, onSignOut, onOpenSettings }: Props) {
  return (
    <aside className="sidebar">
      <div className="sidebar-top" data-tauri-drag-region />
      <div className="org">
        <span className="brand-mark small" aria-hidden>
          ◆
        </span>
        <span className="org-name">{session.organization}</span>
      </div>

      <nav className="nav">
        {NAV.map((item) => (
          <button
            key={item.view}
            className={`nav-item ${view === item.view ? "is-active" : ""}`}
            onClick={() => onSelect(item.view)}
          >
            <span className="nav-icon" aria-hidden>
              {item.icon}
            </span>
            <span>{item.label}</span>
            {counts[item.view] != null && <span className="nav-count">{counts[item.view]}</span>}
          </button>
        ))}
      </nav>

      <div className="user">
        <span className="avatar">{initials(session.displayName)}</span>
        <span className="user-name" title={session.displayName}>
          {session.displayName}
        </span>
        <button
          className="btn btn-ghost btn-sm"
          onClick={onOpenSettings}
          title="Settings (⌘,)"
          aria-label="Settings"
        >
          <span aria-hidden>⚙</span>
        </button>
        <button className="btn btn-ghost btn-sm" onClick={onSignOut}>
          Sign out
        </button>
      </div>
    </aside>
  );
}
