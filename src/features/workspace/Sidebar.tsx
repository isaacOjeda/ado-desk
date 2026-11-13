import type { Session } from "../../lib/api";
import type { SavedView } from "../../lib/settings";
import { initials } from "../../lib/format";
import { OrgSwitcher } from "../organizations/OrgSwitcher";
import { MOD_KEY } from "../../lib/platform";

export type View = "inbox" | "workItems" | "pullRequests";

interface Props {
  session: Session;
  view: View;
  counts: Record<View, number | null>;
  onSelect: (view: View) => void;
  onSignOut: () => void;
  onOpenSettings: () => void;
  onSearch: () => void;
  assistantOpen: boolean;
  onToggleAssistant: () => void;
  /** Views of the active organization plus the starters. */
  views: SavedView[];
  /** The view whose filters equal the current ones of the list on screen. */
  activeViewId: string | null;
  onApplyView: (view: SavedView) => void;
  onRemoveView: (view: SavedView) => void;
}

const NAV: { view: View; label: string; icon: string }[] = [
  { view: "inbox", label: "Inbox", icon: "◎" },
  { view: "workItems", label: "My work items", icon: "▤" },
  { view: "pullRequests", label: "Pull requests", icon: "⇄" },
];

export function Sidebar({
  session,
  view,
  counts,
  onSelect,
  onSignOut,
  onOpenSettings,
  assistantOpen,
  onToggleAssistant,
  onSearch,
  views,
  activeViewId,
  onApplyView,
  onRemoveView,
}: Props) {
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
      <button
        className={`nav-item nav-search${assistantOpen ? " is-active" : ""}`}
        onClick={onToggleAssistant}
        title={`Assistant (${MOD_KEY}J)`}
        aria-pressed={assistantOpen}
      >
        <span className="nav-icon" aria-hidden>
          ✦
        </span>
        <span>Assistant</span>
        <kbd>{MOD_KEY}J</kbd>
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

      {views.length > 0 && (
        <nav className="nav nav-views" aria-label="Saved views">
          <p className="nav-heading">Views</p>
          {views.map((saved) => (
            <div key={saved.id} className="nav-view">
              <button
                className={`nav-item ${activeViewId === saved.id ? "is-active" : ""}`}
                onClick={() => onApplyView(saved)}
                title={saved.name}
              >
                <span className="nav-icon" aria-hidden>
                  {saved.scope === "workItems" ? "▤" : "⇄"}
                </span>
                <span className="nav-view-name">{saved.name}</span>
              </button>
              <button
                className="nav-view-remove"
                onClick={() => onRemoveView(saved)}
                title="Remove view"
                aria-label={`Remove view ${saved.name}`}
              >
                ×
              </button>
            </div>
          ))}
        </nav>
      )}

      <div className="user">
        <span className="avatar">{initials(session.displayName)}</span>
        <span className="user-name" title={session.displayName}>
          {session.displayName}
        </span>
        <button
          className="btn btn-ghost btn-sm"
          onClick={onOpenSettings}
          title={`Settings (${MOD_KEY},)`}
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
