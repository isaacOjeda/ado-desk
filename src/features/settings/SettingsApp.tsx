import { useCallback, useEffect, useState } from "react";
import { api, onSessionChanged, subscribe, type OrganizationSummary, type Session } from "../../lib/api";
import { DEFAULT_SETTINGS, settingsApi, type Settings } from "../../lib/settings";
import { ErrorBanner } from "../../components/ui";
import { AppearanceSection } from "./AppearanceSection";
import { ProfileSection } from "./ProfileSection";
import "./settings.css";

type Section = "profile" | "appearance";

const SECTIONS: { id: Section; label: string; icon: string }[] = [
  { id: "profile", label: "Profile", icon: "◉" },
  { id: "appearance", label: "Appearance", icon: "◐" },
];

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Root of the Settings window (`index.html#settings`). */
export function SettingsApp() {
  const [section, setSection] = useState<Section>("profile");
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [session, setSession] = useState<Session | null | undefined>(undefined);
  const [organizations, setOrganizations] = useState<OrganizationSummary[] | undefined>(undefined);
  const [busyOrganization, setBusyOrganization] = useState<string | null>(null);
  const [signingOut, setSigningOut] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    settingsApi
      .get()
      .then(setSettings)
      .catch(() => setSettings(DEFAULT_SETTINGS));
    return subscribe(() => settingsApi.onChanged(setSettings));
  }, []);

  const reloadOrganizations = useCallback(() => {
    api
      .listOrganizations()
      .then(setOrganizations)
      .catch((err) => {
        setOrganizations([]);
        setError(`Could not load organizations: ${message(err)}`);
      });
  }, []);

  useEffect(() => {
    settingsApi
      .currentSession()
      .then(setSession)
      .catch(() => setSession(null));
    reloadOrganizations();
    return subscribe(() =>
      onSessionChanged((next) => {
        setSession(next);
        reloadOrganizations();
      }),
    );
  }, [reloadOrganizations]);

  const switchOrganization = useCallback(
    (organization: string) => {
      setBusyOrganization(organization);
      setError(null);
      api
        .switchOrganization(organization)
        .then(setSession)
        .catch((err) => setError(`Could not switch to ${organization}: ${message(err)}`))
        .finally(() => {
          setBusyOrganization(null);
          reloadOrganizations();
        });
    },
    [reloadOrganizations],
  );

  const removeOrganization = useCallback(
    (organization: string) => {
      setBusyOrganization(organization);
      setError(null);
      api
        .removeOrganization(organization)
        .then(setSession)
        .catch((err) => setError(`Could not remove ${organization}: ${message(err)}`))
        .finally(() => {
          setBusyOrganization(null);
          // Rust is the source of truth after a partial failure.
          settingsApi.currentSession().then(setSession).catch(() => {});
          reloadOrganizations();
        });
    },
    [reloadOrganizations],
  );

  const organizationAdded = useCallback(
    (next: Session) => {
      setError(null);
      setSession(next);
      reloadOrganizations();
    },
    [reloadOrganizations],
  );

  const changeSettings = useCallback((next: Settings) => {
    setError(null);
    setSettings(next);
    // The stored (normalized) value comes back through `settings-changed`, which keeps
    // rapid successive changes in the order Rust persisted them.
    settingsApi.update(next).catch((err) => {
        setError(`Could not save settings: ${message(err)}`);
        // Resync with what is actually stored.
      settingsApi.get().then(setSettings).catch(() => {});
    });
  }, []);

  // Signs out of the active organization; Rust may switch to the next one.
  const signOut = useCallback(() => {
    setSigningOut(true);
    setError(null);
    api
      .logout()
      .then(setSession)
      .catch((err) => {
        setSession(null);
        setError(`Signed out, but the saved token could not be removed: ${message(err)}`);
      })
      .finally(() => {
        setSigningOut(false);
        reloadOrganizations();
      });
  }, [reloadOrganizations]);

  const title = SECTIONS.find((s) => s.id === section)!.label;

  return (
    <div className="settings">
      <aside className="settings-nav">
        <div className="settings-nav-top" data-tauri-drag-region />
        <nav className="settings-nav-list" aria-label="Settings sections">
          {SECTIONS.map((item) => (
            <button
              key={item.id}
              className={`settings-nav-item ${section === item.id ? "is-active" : ""}`}
              aria-current={section === item.id ? "page" : undefined}
              onClick={() => setSection(item.id)}
            >
              <span className="settings-nav-icon" aria-hidden>
                {item.icon}
              </span>
              <span>{item.label}</span>
            </button>
          ))}
        </nav>
      </aside>

      <main className="settings-content">
        <header className="settings-header" data-tauri-drag-region>
          <h1>{title}</h1>
        </header>
        <div className="settings-body">
          {error && <ErrorBanner message={error} />}
          {section === "profile" ? (
            <ProfileSection
              session={session}
              organizations={organizations}
              busyOrganization={busyOrganization}
              signingOut={signingOut}
              onSignOut={signOut}
              onSwitch={switchOrganization}
              onRemove={removeOrganization}
              onAdded={organizationAdded}
            />
          ) : (
            <AppearanceSection settings={settings} onChange={changeSettings} />
          )}
        </div>
      </main>
    </div>
  );
}
