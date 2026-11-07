import type { Session } from "../../lib/api";
import { initials } from "../../lib/format";
import { OrgSwitcher } from "../organizations/OrgSwitcher";
import { MOD_KEY } from "../search/QuickSearch";

export type View = "inbox" | "workItems" | "pullRequests";

interface Props {
  session: Session;
  view: View;
  counts: Record<View, number | null>;
  onSelect: (view: View) => void;
  onSignOut: () => void;
  onOpenSettings: () => void;
  onSearch: () => void;
}

const NAV: { view: View; label: string; icon: string }[] = [
  { view: "inbox", label: "Inbox", icon: "◎" },
  { view: "workItems", label: "My work items", icon: "▤" },
  { view: "pullRequests", label: "Pull requests", icon: "⇄" },
];

export function Sidebar({ session, view, counts, onSelect, onSignOut, onOpenSettings, onSearch }: Props) {
  return (
    <aside className="sidebar">
      <div className="sidebar-top" data-tauri-drag-region />
      <OrgSwitcher session={session} />

      <button className="nav-item nav-search" onClick={onSearch} title={`Search (${MOD_KEY}K)`}>
        <span className="nav-icon" aria-hidden>
          ⌕
        </span>
        <span>Search</span>
        <kbd>{MOD_KEY}K</kbd>
      </button>

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
            {item.view === "inbox" ? (
              !!counts.inbox && <span className="nav-count nav-unread">{counts.inbox}</span>
            ) : (
              counts[item.view] != null && <span className="nav-count">{counts[item.view]}</span>
            )}
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
        <button className="btn btn-ghost btn-sm" onClick={onSignOut} title={`Sign out of ${session.organization}`}>
          Sign out
        </button>
      </div>
    </aside>
  );
}
