import { useCallback, useEffect, useState } from "react";
import { ApiError, api, onSessionChanged, subscribe, type Session } from "./lib/api";
import { LoginView } from "./features/auth/LoginView";
import { Workspace } from "./features/workspace/Workspace";
import { onAddOrganizationRequested } from "./features/organizations/events";
import { Spinner } from "./components/ui";
import "./styles.css";

type AuthState =
  | { status: "restoring" }
  | { status: "signedOut"; notice?: string }
  | { status: "signedIn"; session: Session };

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export default function App() {
  const [auth, setAuth] = useState<AuthState>({ status: "restoring" });
  const [addingOrganization, setAddingOrganization] = useState(false);

  useEffect(() => {
    api
      .restoreSession()
      .then((session) => setAuth(session ? { status: "signedIn", session } : { status: "signedOut" }))
      .catch((err) =>
        setAuth({
          status: "signedOut",
          notice:
            err instanceof ApiError && err.kind === "unauthorized"
              ? "Your saved token is no longer valid. Please enter a new one."
              : `Could not restore your session: ${message(err)}`,
        }),
      );
  }, []);

  // Rust broadcasts every change of the active organization (from any window): follow it.
  useEffect(
    () =>
      subscribe(() =>
        onSessionChanged((session) => {
          if (session) {
            setAuth({ status: "signedIn", session });
          } else {
            setAuth((current) => (current.status === "signedIn" ? { status: "signedOut" } : current));
          }
        }),
      ),
    [],
  );

  useEffect(() => onAddOrganizationRequested(() => setAddingOrganization(true)), []);

  // The add-organization screen belongs to a signed-in workspace.
  useEffect(() => {
    if (auth.status !== "signedIn") setAddingOrganization(false);
  }, [auth.status]);

  // Signing out removes the active organization; Rust may switch to the next one.
  const signOut = useCallback(() => {
    api
      .logout()
      .then((next) => setAuth(next ? { status: "signedIn", session: next } : { status: "signedOut" }))
      .catch((err) =>
        setAuth({
          status: "signedOut",
          notice: `Signed out, but the saved token could not be removed: ${message(err)}`,
        }),
      );
  }, []);

  const expired = useCallback(() => {
    setAuth({ status: "signedOut", notice: "Your session expired. Please sign in again." });
  }, []);

  const closeAddOrganization = useCallback(() => setAddingOrganization(false), []);

  switch (auth.status) {
    case "restoring":
      return (
        <div className="splash" data-tauri-drag-region>
          <Spinner />
        </div>
      );
    case "signedOut":
      return (
        <LoginView notice={auth.notice} onSignedIn={(session) => setAuth({ status: "signedIn", session })} />
      );
    case "signedIn":
      return (
        <>
          {/* Keyed by organization so every list reloads after a switch. */}
          <Workspace
            key={auth.session.organization}
            session={auth.session}
            onSignOut={signOut}
            onUnauthorized={expired}
          />
          {addingOrganization && (
            <div className="org-overlay">
              <LoginView
                mode="addOrganization"
                onSignedIn={(session) => {
                  setAddingOrganization(false);
                  setAuth({ status: "signedIn", session });
                }}
                onCancel={closeAddOrganization}
              />
            </div>
          )}
        </>
      );
  }
}
