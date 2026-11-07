import { useState } from "react";
import type { OrganizationSummary, Session } from "../../lib/api";
import { Badge, Spinner } from "../../components/ui";
import { AddOrganizationForm } from "./AddOrganizationForm";
import "./organizations.css";

interface Props {
  /** `undefined` while loading. */
  organizations: OrganizationSummary[] | undefined;
  /** Organization with an operation in flight, if any. */
  busy: string | null;
  onSwitch: (organization: string) => void;
  onRemove: (organization: string) => void;
  onAdded: (session: Session) => void;
}

/** Settings list of every configured organization: switch, remove (confirmed) and add. */
export function OrganizationsCard({ organizations, busy, onSwitch, onRemove, onAdded }: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);

  return (
    <section className="settings-card">
      <h2 className="settings-card-title">Organizations</h2>

      {organizations === undefined ? (
        <div className="org-list-loading">
          <Spinner />
        </div>
      ) : organizations.length === 0 ? (
        <p className="org-list-empty">No organizations yet.</p>
      ) : (
        <ul className="org-list">
          {organizations.map((org) => (
            <li key={org.organization} className="org-list-row">
              <div className="org-list-names">
                <span className="org-list-name">{org.organization}</span>
                {org.displayName && <span className="org-list-user">{org.email ?? org.displayName}</span>}
              </div>

              {confirming === org.organization ? (
                <div className="org-list-actions" role="group" aria-label={`Remove ${org.organization}?`}>
                  <span className="org-list-confirm">Remove and delete its token?</span>
                  <button className="btn btn-ghost btn-sm" onClick={() => setConfirming(null)}>
                    Cancel
                  </button>
                  <button
                    className="btn btn-danger btn-sm"
                    onClick={() => {
                      setConfirming(null);
                      onRemove(org.organization);
                    }}
                  >
                    Remove
                  </button>
                </div>
              ) : (
                <div className="org-list-actions">
                  {busy === org.organization && <Spinner />}
                  {org.active ? (
                    <Badge tone="green">Active</Badge>
                  ) : (
                    <button
                      className="btn btn-ghost btn-sm"
                      onClick={() => onSwitch(org.organization)}
                      disabled={busy !== null}
                    >
                      Switch
                    </button>
                  )}
                  <button
                    className="btn btn-ghost btn-sm"
                    onClick={() => setConfirming(org.organization)}
                    disabled={busy !== null}
                  >
                    Remove
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}

      {adding ? (
        <div className="org-list-add">
          <AddOrganizationForm
            submitLabel="Add organization"
            onAdded={(session) => {
              setAdding(false);
              onAdded(session);
            }}
            onCancel={() => setAdding(false)}
          />
        </div>
      ) : (
        <div className="org-list-footer">
          <button className="btn btn-ghost btn-sm" onClick={() => setAdding(true)}>
            + Add organization
          </button>
        </div>
      )}
    </section>
  );
}
