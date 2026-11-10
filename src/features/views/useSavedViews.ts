import { useEffect, useState } from "react";
import { settingsApi, type SavedView, type Settings } from "../../lib/settings";

/**
 * Saved views from the Rust settings store, kept current through `settings-changed`.
 * `null` until loaded, so removed starters never flash in. `accept` applies a settings value
 * returned by `saveView` / `deleteView` without waiting for the event.
 */
export function useSavedViews(): [SavedView[] | null, (settings: Settings) => void] {
  const [views, setViews] = useState<SavedView[] | null>(null);

  useEffect(() => {
    let active = true;
    settingsApi
      .get()
      .then((settings) => active && setViews(settings.views))
      .catch(() => active && setViews([]));

    let unlisten: (() => void) | undefined;
    try {
      settingsApi
        .onChanged((settings) => active && setViews(settings.views))
        .then((stop) => (active ? (unlisten = stop) : stop()))
        .catch(() => {});
    } catch {
      // Not running inside Tauri: no live updates.
    }
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  return [views, (settings: Settings) => setViews(settings.views)];
}
