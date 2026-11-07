// Inbox view: what the background checks found (review requests, assignments, comments).
// Opening an item marks it read and opens it like a quick search result.
import { useState } from "react";
import { EmptyState, ErrorBanner, Spinner } from "../../components/ui";
import { timeAgo } from "../../lib/format";
import { ApiError } from "../../lib/api";
import { requestOpen } from "../search/events";
import { inboxApi, type InboxItem, type InboxKind } from "./api";
import "./inbox.css";

const ICONS: Record<InboxKind, string> = { reviewRequest: "⇄", assignment: "▤", comment: "✎" };

interface Props {
  items: InboxItem[] | null;
  query: string;
}

function message(err: unknown): string {
  return err instanceof ApiError ? err.message : "Something went wrong. Try again.";
}

export function InboxView({ items, query }: Props) {
  const [error, setError] = useState<string | null>(null);
  const q = query.trim().toLowerCase();
  const visible = (items ?? []).filter((item) => !q || `${item.title} ${item.detail}`.toLowerCase().includes(q));

  const open = (item: InboxItem) => {
    setError(null);
    if (!item.read) inboxApi.markRead(item.id).catch((err) => setError(message(err)));
    requestOpen(item.target);
  };

  if (items === null) {
    return (
      <div className="loading">
        <Spinner />
      </div>
    );
  }
  return (
    <>
      {error && <ErrorBanner message={error} />}
      {visible.length === 0 ? (
        q ? (
          <EmptyState title="Nothing matches this filter" />
        ) : (
          <EmptyState
            title="You're all caught up"
            hint="New review requests, assignments and comments show up here. Checks run in the background; see Settings > Notifications."
          />
        )
      ) : (
        <ul className="list">
          {visible.map((item) => (
            <li key={item.id}>
              <button className={`row inbox-row ${item.read ? "" : "is-unread"}`} onClick={() => open(item)}>
                <span className="inbox-dot" aria-label={item.read ? undefined : "Unread"} />
                <span className="inbox-icon" aria-hidden>
                  {ICONS[item.kind]}
                </span>
                <span className="row-main">
                  <span className="row-title">{item.title}</span>
                  <span className="row-meta">{item.detail}</span>
                </span>
                <span className="row-time">{timeAgo(new Date(item.at).toISOString())}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
