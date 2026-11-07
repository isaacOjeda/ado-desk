import { useState, type FormEvent } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ApiError, api, type Session } from "../../lib/api";
import { Spinner } from "../../components/ui";

interface Props {
  notice?: string;
  onSignedIn: (session: Session) => void;
}

const PAT_DOCS =
  "https://learn.microsoft.com/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate";

export function LoginView({ notice, onSignedIn }: Props) {
  const [organization, setOrganization] = useState("");
  const [pat, setPat] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const session = await api.login(organization, pat);
      setPat("");
      onSignedIn(session);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="login" data-tauri-drag-region>
      <form className="login-card" onSubmit={submit}>
        <div className="brand-mark" aria-hidden>
          ◆
        </div>
        <h1>ADO Desk</h1>
        <p className="muted">Your Azure Boards work items and pull requests, in one place.</p>

        {notice && <p className="notice">{notice}</p>}

        <label className="field">
          <span>Organization</span>
          <input
            autoFocus
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
            placeholder="Scopes: Work Items, Code, Project (Read)"
            value={pat}
            onChange={(e) => setPat(e.target.value)}
            autoComplete="off"
            required
          />
        </label>

        {error && <p className="form-error">{error}</p>}

        <button className="btn btn-primary" type="submit" disabled={busy}>
          {busy ? <Spinner /> : "Connect"}
        </button>

        <p className="fine-print">
          The token is stored in your system keychain and only used to call Azure DevOps.{" "}
          <a
            href={PAT_DOCS}
            onClick={(e) => {
              e.preventDefault();
              void openUrl(PAT_DOCS);
            }}
          >
            How to create a PAT
          </a>
        </p>
      </form>
    </div>
  );
}
