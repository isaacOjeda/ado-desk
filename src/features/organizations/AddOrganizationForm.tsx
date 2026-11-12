import { useState, type FormEvent } from "react";
import { ApiError, api, type Session } from "../../lib/api";
import { Spinner } from "../../components/ui";
import "./organizations.css";

interface Props {
  submitLabel: string;
  onAdded: (session: Session) => void;
  onCancel?: () => void;
  autoFocus?: boolean;
}

/**
 * Organization + PAT form. Rust validates the token against Azure DevOps before storing it
 * in the keychain and makes the organization active; adding an existing one replaces its PAT.
 */
export function AddOrganizationForm({ submitLabel, onAdded, onCancel, autoFocus = true }: Props) {
  const [organization, setOrganization] = useState("");
  const [pat, setPat] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const session = await api.addOrganization(organization, pat);
      setPat("");
      onAdded(session);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className="org-form" onSubmit={submit}>
      <label className="field">
        <span>Organization</span>
        <input
          autoFocus={autoFocus}
          placeholder="contoso or https://dev.azure.com/contoso"
          value={organization}
          onChange={(e) => setOrganization(e.target.value)}
          required
        />
      </label>

      <label className="field">
        <span>Personal access token</span>
        <input
          type="password"
          placeholder="Scopes: Work Items, Code, Project (Read); Build (Read), Work Items and Code (Write) optional"
          value={pat}
          onChange={(e) => setPat(e.target.value)}
          autoComplete="off"
          required
        />
      </label>

      {error && <p className="form-error">{error}</p>}

      <div className="org-form-actions">
        {onCancel && (
          <button className="btn btn-ghost" type="button" onClick={onCancel} disabled={busy}>
            Cancel
          </button>
        )}
        <button className="btn btn-primary" type="submit" disabled={busy}>
          {busy ? <Spinner /> : submitLabel}
        </button>
      </div>
    </form>
  );
}
