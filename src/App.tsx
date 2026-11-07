import { useCallback, useEffect, useState } from "react";
import { ApiError, api, type Session } from "./lib/api";
import { LoginView } from "./features/auth/LoginView";
import { Workspace } from "./features/workspace/Workspace";
import { Spinner } from "./components/ui";
import "./styles.css";

type AuthState =
  | { status: "restoring" }
  | { status: "signedOut"; notice?: string }
  | { status: "signedIn"; session: Session };

export default function App() {
  const [auth, setAuth] = useState<AuthState>({ status: "restoring" });

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
              : `Could not restore your session: ${err instanceof Error ? err.message : String(err)}`,
        }),
      );
  }, []);

  const signOut = useCallback(() => {
    void api.logout().finally(() => setAuth({ status: "signedOut" }));
  }, []);

  const expired = useCallback(() => {
    setAuth({ status: "signedOut", notice: "Your session expired. Please sign in again." });
  }, []);

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
      return <Workspace session={auth.session} onSignOut={signOut} onUnauthorized={expired} />;
  }
}
