import { useCallback, useEffect, useState } from "react";
import { api, onSessionChanged, subscribe, type Session } from "../../lib/api";
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
  const [signingOut, setSigningOut] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    settingsApi
      .get()
      .then(setSettings)
      .catch(() => setSettings(DEFAULT_SETTINGS));
    return subscribe(() => settingsApi.onChanged(setSettings));
  }, []);

  useEffect(() => {
    settingsApi
      .currentSession()
      .then(setSession)
      .catch(() => setSession(null));
    return subscribe(() => onSessionChanged(setSession));
  }, []);

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

  const signOut = useCallback(() => {
    setSigningOut(true);
    setError(null);
    api
      .logout()
      .catch((err) => setError(`Signed out, but the saved token could not be removed: ${message(err)}`))
      .finally(() => {
        setSession(null);
        setSigningOut(false);
      });
  }, []);

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
            <ProfileSection session={session} signingOut={signingOut} onSignOut={signOut} />
          ) : (
            <AppearanceSection settings={settings} onChange={changeSettings} />
          )}
        </div>
      </main>
    </div>
  );
}
