// The floating search window (index.html#search), shown by the global shortcut. It searches
// like the Cmd+K palette and hands the picked item to the main window.
import { useEffect, useState } from "react";
import { onSessionChanged, subscribe, type Session } from "../../lib/api";
import { settingsApi } from "../../lib/settings";
import { QuickSearch } from "./QuickSearch";
import { searchApi, type SearchHit } from "./api";
import "./search.css";

export function SearchBarApp() {
  const [session, setSession] = useState<Session | null | undefined>(undefined);
  // Remounting the palette on every show clears the query and rereads the recent items.
  const [shown, setShown] = useState(0);

  useEffect(() => {
    settingsApi
      .currentSession()
      .then(setSession)
      .catch(() => setSession(null));
  }, []);
  useEffect(() => subscribe(() => onSessionChanged(setSession)), []);
  useEffect(() => subscribe(() => searchApi.onBarShown(() => setShown((n) => n + 1))), []);

  const close = () => void searchApi.hideBar();

  if (session === undefined) return null;
  if (session === null) {
    return (
      <div className="qs-signed-out" onKeyDown={(e) => e.key === "Escape" && close()}>
        <p>Sign in to ADO Desk to search work items and pull requests.</p>
        <button className="btn btn-ghost" autoFocus onClick={close}>
          Close
        </button>
      </div>
    );
  }
  return (
    <QuickSearch
      key={shown}
      variant="bar"
      organization={session.organization}
      onOpen={(hit: SearchHit) => searchApi.openFromBar(hit)}
      onClose={close}
    />
  );
}
