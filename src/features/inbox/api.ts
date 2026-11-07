// Inbox of notifications found by the background checks (src-tauri/src/notifications).
import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, subscribe } from "../../lib/api";
import type { SearchHit } from "../search/api";

export type InboxKind = "reviewRequest" | "assignment" | "comment";

/** Mirrors `notifications::store::InboxItem`. */
export interface InboxItem {
  id: string;
  kind: InboxKind;
  title: string;
  detail: string;
  /** Unix time in milliseconds. */
  at: number;
  read: boolean;
  target: SearchHit;
}

export const INBOX_CHANGED_EVENT = "inbox-changed";

export const inboxApi = {
  list: () => call<InboxItem[]>("list_inbox"),
  /** Marks one item read, or every item when `id` is omitted. */
  markRead: (id?: string) => call<InboxItem[]>("mark_inbox_read", { id: id ?? null }),
  checkNow: () => call<void>("check_notifications_now"),
  onChanged: (handler: (items: InboxItem[]) => void): Promise<UnlistenFn> =>
    listen<InboxItem[]>(INBOX_CHANGED_EVENT, (event) => handler(event.payload)),
};

/** The Inbox, kept current by `inbox-changed`. Null until the first load. */
export function useInbox(): InboxItem[] | null {
  const [items, setItems] = useState<InboxItem[] | null>(null);
  useEffect(() => {
    inboxApi
      .list()
      .then(setItems)
      .catch(() => setItems([]));
    return subscribe(() => inboxApi.onChanged(setItems));
  }, []);
  return items;
}
