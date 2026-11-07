import { useEffect, useRef, useState } from "react";
import { ApiError, api, type OrganizationSummary, type Session } from "../../lib/api";
import { settingsApi } from "../../lib/settings";
import { Spinner } from "../../components/ui";
import { requestAddOrganization } from "./events";
import "./organizations.css";

function message(err: unknown): string {
  return err instanceof ApiError || err instanceof Error ? err.message : String(err);
}

/**
 * Active organization at the top of the sidebar; opens a menu to switch, add or manage
 * organizations. Switching goes through Rust, whose `session-changed` event updates App.
 */
export function OrgSwitcher({ session }: { session: Session }) {
  const [open, setOpen] = useState(false);
  const [organizations, setOrganizations] = useState<OrganizationSummary[] | null>(null);
  const [switching, setSwitching] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    setError(null);
    api
      .listOrganizations()
      .then(setOrganizations)
      .catch((err) => setError(`Could not load organizations: ${message(err)}`));

    const onPointerDown = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    // Capture phase so Escape closes only the menu, not a detail view underneath.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [open]);

  function switchTo(organization: string) {
    setSwitching(organization);
    setError(null);
    api
      .switchOrganization(organization)
      .then(() => setOpen(false))
      .catch((err) => setError(`Could not switch to ${organization}: ${message(err)}`))
      .finally(() => setSwitching(null));
  }

  function addOrganization() {
    setOpen(false);
    requestAddOrganization();
  }

  function manage() {
    setOpen(false);
    settingsApi.openWindow().catch((err: unknown) => console.error("Could not open settings", err));
  }

  return (
    <div className="org-switcher" ref={root}>
      <button
        className="org-switcher-button"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        title="Switch organization"
      >
        <span className="brand-mark small" aria-hidden>
          ◆
        </span>
        <span className="org-name">{session.organization}</span>
        <span className="org-switcher-chevron" aria-hidden>
          ▾
        </span>
      </button>

      {open && (
        <div className="org-menu" role="menu" aria-label="Organizations">
          {organizations === null && !error && (
            <div className="org-menu-loading">
              <Spinner />
            </div>
          )}
          {organizations?.map((org) => {
            const active = org.organization === session.organization;
            return (
              <button
                key={org.organization}
                className={`org-menu-item ${active ? "is-active" : ""}`}
                role="menuitemradio"
                aria-checked={active}
                disabled={switching !== null}
                onClick={() => (active ? setOpen(false) : switchTo(org.organization))}
              >
                <span className="org-menu-check" aria-hidden>
                  {active ? "✓" : ""}
                </span>
                <span className="org-menu-label">{org.organization}</span>
                {switching === org.organization && <Spinner />}
              </button>
            );
          })}
          {error && <p className="org-menu-error">{error}</p>}
          <div className="org-menu-separator" role="separator" />
          <button className="org-menu-item" role="menuitem" onClick={addOrganization}>
            <span className="org-menu-check" aria-hidden>
              +
            </span>
            <span className="org-menu-label">Add organization…</span>
          </button>
          <button className="org-menu-item" role="menuitem" onClick={manage}>
            <span className="org-menu-check" aria-hidden>
              ⚙
            </span>
            <span className="org-menu-label">Manage in Settings</span>
          </button>
        </div>
      )}
    </div>
  );
}
