import { openUrl } from "@tauri-apps/plugin-opener";
import type { OrganizationSummary, Session } from "../../lib/api";
import { Avatar, EmptyState, Spinner } from "../../components/ui";
import { OrganizationsCard } from "../organizations/OrganizationsCard";

interface Props {
  /** `undefined` while loading, `null` when signed out. */
  session: Session | null | undefined;
  /** `undefined` while loading. */
  organizations: OrganizationSummary[] | undefined;
  /** Organization with a switch or removal in flight. */
  busyOrganization: string | null;
  signingOut: boolean;
  onSignOut: () => void;
  onSwitch: (organization: string) => void;
  onRemove: (organization: string) => void;
  onAdded: (session: Session) => void;
}

export function organizationUrl(organization: string): string {
  return `https://dev.azure.com/${encodeURIComponent(organization)}`;
}

export function ProfileSection({
  session,
  organizations,
  busyOrganization,
  signingOut,
  onSignOut,
  onSwitch,
  onRemove,
  onAdded,
}: Props) {
  if (session === undefined) {
    return (
      <div className="settings-loading">
        <Spinner />
      </div>
    );
  }

  const organizationsCard = (
    <OrganizationsCard
      organizations={organizations}
      busy={busyOrganization}
      onSwitch={onSwitch}
      onRemove={onRemove}
      onAdded={onAdded}
    />
  );

  if (session === null) {
    return (
      <div className="settings-stack">
        <EmptyState
          title="You're not signed in"
          hint="Switch to one of your organizations or add one with a personal access token."
        />
        {organizationsCard}
      </div>
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
        <h2 className="settings-card-title">Active organization</h2>
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

      {organizationsCard}

      <div className="settings-actions">
        <button className="btn btn-danger" onClick={onSignOut} disabled={signingOut}>
          {signingOut ? <Spinner /> : null}
          Sign out of {session.organization}
        </button>
      </div>
    </div>
  );
}
