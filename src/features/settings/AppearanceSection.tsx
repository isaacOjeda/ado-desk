// STUB: owned by task T3 (odd/tasks/appearance-settings.md). Rendered by the Settings window (T2).
import type { Settings } from "../../lib/settings";

export interface AppearanceSectionProps {
  settings: Settings;
  onChange: (next: Settings) => void;
}

export function AppearanceSection(_props: AppearanceSectionProps) {
  return <p>Appearance</p>;
}
