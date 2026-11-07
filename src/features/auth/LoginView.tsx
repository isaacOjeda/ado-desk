import { useEffect } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Session } from "../../lib/api";
import { AddOrganizationForm } from "../organizations/AddOrganizationForm";

interface Props {
  notice?: string;
  /** `addOrganization` is shown over the workspace from the organization switcher. */
  mode?: "signIn" | "addOrganization";
  onSignedIn: (session: Session) => void;
  /** Shows a Cancel button (and closes on Escape); used in `addOrganization` mode. */
  onCancel?: () => void;
}

const PAT_DOCS =
  "https://learn.microsoft.com/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate";

export function LoginView({ notice, mode = "signIn", onSignedIn, onCancel }: Props) {
  const adding = mode === "addOrganization";

  useEffect(() => {
    if (!onCancel) return;
    // Capture phase so the workspace underneath does not also handle Escape.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      onCancel();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onCancel]);

  return (
    <div className="login" data-tauri-drag-region>
      <div className="login-card">
        <div className="brand-mark" aria-hidden>
          ◆
        </div>
        <h1>{adding ? "Add organization" : "ADO Desk"}</h1>
        <p className="muted">
          {adding
            ? "Connect another Azure DevOps organization with its own personal access token."
            : "Your Azure Boards work items and pull requests, in one place."}
        </p>

        {notice && <p className="notice">{notice}</p>}

        <AddOrganizationForm
          submitLabel={adding ? "Add organization" : "Connect"}
          onAdded={onSignedIn}
          onCancel={onCancel}
        />

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
      </div>
    </div>
  );
}
