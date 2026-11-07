import { openUrl } from "@tauri-apps/plugin-opener";
import type { Session } from "../../lib/api";
import { Avatar, EmptyState, Spinner } from "../../components/ui";

interface Props {
  /** `undefined` while loading, `null` when signed out. */
  session: Session | null | undefined;
  signingOut: boolean;
  onSignOut: () => void;
}

export function organizationUrl(organization: string): string {
  return `https://dev.azure.com/${encodeURIComponent(organization)}`;
}

export function ProfileSection({ session, signingOut, onSignOut }: Props) {
  if (session === undefined) {
    return (
      <div className="settings-loading">
        <Spinner />
      </div>
    );
  }

  if (session === null) {
    return (
      <EmptyState
        title="You're not signed in"
        hint="Sign in from the main window with your organization and a personal access token."
      />
    );
  }

  const url = organizationUrl(session.organization);

  return (
    <div className="settings-stack">
      <section className="settings-card profile-identity">
        <Avatar name={session.displayName} size={56} />
        <div className="profile-names">
          <p className="profile-name">{session.displayName}</p>
          {session.email && <p className="profile-email">{session.email}</p>}
        </div>
      </section>

      <section className="settings-card">
        <h2 className="settings-card-title">Organization</h2>
        <dl className="settings-rows">
          <div className="settings-row">
            <dt>Name</dt>
            <dd>{session.organization}</dd>
          </div>
          <div className="settings-row">
            <dt>URL</dt>
            <dd>
              <button className="settings-link" onClick={() => void openUrl(url)} title="Open in browser">
                {url} ↗
              </button>
            </dd>
          </div>
          <div className="settings-row">
            <dt>Authentication</dt>
            <dd>Personal access token · stored in your system keychain</dd>
          </div>
        </dl>
      </section>

      <div className="settings-actions">
        <button className="btn btn-danger" onClick={onSignOut} disabled={signingOut}>
          {signingOut ? <Spinner /> : null}
          Sign out
        </button>
      </div>
    </div>
  );
}
