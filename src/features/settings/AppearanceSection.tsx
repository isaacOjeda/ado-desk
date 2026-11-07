// Appearance settings: mode, the light/dark theme choices and translucency. Rendered by the Settings window.
import type { CSSProperties } from "react";
import type { AppearanceSettings, Settings, ThemeMode } from "../../lib/settings";
import { useThemes } from "../../themes/ThemeRoot";
import { themeCssVariables } from "../../themes/tokens";
import { DEFAULT_THEME_IDS, type Theme, type ThemeKind } from "../../themes/types";
import "./appearance.css";

export interface AppearanceSectionProps {
  settings: Settings;
  onChange: (next: Settings) => void;
}

const MODES: { value: ThemeMode; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function AppearanceSection({ settings, onChange }: AppearanceSectionProps) {
  const { themes, kind: effectiveKind } = useThemes();
  const appearance = settings.appearance;
  const update = (patch: Partial<AppearanceSettings>) =>
    onChange({ ...settings, appearance: { ...appearance, ...patch } });

  return (
    <div className="appearance">
      <section className="appearance-row">
        <div className="appearance-text">
          <h3 className="appearance-label" id="appearance-mode">
            Mode
          </h3>
          <p className="appearance-hint">System follows your operating system's light or dark appearance.</p>
        </div>
        <div className="segmented" role="radiogroup" aria-labelledby="appearance-mode">
          {MODES.map((mode) => (
            <button
              key={mode.value}
              type="button"
              role="radio"
              aria-checked={appearance.mode === mode.value}
              className={appearance.mode === mode.value ? "is-active" : ""}
              onClick={() => update({ mode: mode.value })}
            >
              {mode.label}
            </button>
          ))}
        </div>
      </section>

      <ThemePicker
        title="Light theme"
        kind="light"
        themes={themes}
        selected={appearance.lightTheme}
        inUse={effectiveKind === "light"}
        onSelect={(id) => update({ lightTheme: id })}
      />
      <ThemePicker
        title="Dark theme"
        kind="dark"
        themes={themes}
        selected={appearance.darkTheme}
        inUse={effectiveKind === "dark"}
        onSelect={(id) => update({ darkTheme: id })}
      />

      <section className="appearance-row">
        <div className="appearance-text">
          <h3 className="appearance-label" id="appearance-translucency">
            Translucency
          </h3>
          <p className="appearance-hint">Let the desktop show through the sidebar and toolbars. Off paints solid surfaces.</p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={appearance.translucency}
          aria-labelledby="appearance-translucency"
          className={`appearance-switch${appearance.translucency ? " is-on" : ""}`}
          onClick={() => update({ translucency: !appearance.translucency })}
        >
          <span className="appearance-switch-thumb" />
        </button>
      </section>

      <p className="appearance-note">Importing themes from Open VSX is coming later.</p>
    </div>
  );
}

interface ThemePickerProps {
  title: string;
  kind: ThemeKind;
  themes: readonly Theme[];
  selected: string;
  inUse: boolean;
  onSelect: (id: string) => void;
}

function ThemePicker({ title, kind, themes, selected, inUse, onSelect }: ThemePickerProps) {
  const options = themes.filter((theme) => theme.kind === kind);
  // An unknown stored id paints the default, so show the default as selected.
  const effective = options.some((t) => t.id === selected) ? selected : DEFAULT_THEME_IDS[kind];
  const labelId = `appearance-${kind}-theme`;

  return (
    <section className="appearance-group">
      <h3 className="appearance-label" id={labelId}>
        {title}
        {inUse && <span className="appearance-in-use">In use</span>}
      </h3>
      <div className="theme-grid" role="radiogroup" aria-labelledby={labelId}>
        {options.map((theme) => (
          <ThemeCard
            key={theme.id}
            theme={theme}
            selected={theme.id === effective}
            isDefault={theme.id === DEFAULT_THEME_IDS[kind]}
            onSelect={() => onSelect(theme.id)}
          />
        ))}
      </div>
    </section>
  );
}

function ThemeCard({
  theme,
  selected,
  isDefault,
  onSelect,
}: {
  theme: Theme;
  selected: boolean;
  isDefault: boolean;
  onSelect: () => void;
}) {
  // The preview re-declares the theme's tokens locally, so it paints with that theme regardless of the active one.
  const previewVars = themeCssVariables(theme.colors) as CSSProperties;

  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      className={`theme-card${selected ? " is-selected" : ""}`}
      onClick={onSelect}
    >
      <span className="theme-preview" style={previewVars} aria-hidden>
        <span className="theme-preview-sidebar">
          <span className="theme-preview-nav is-active" />
          <span className="theme-preview-nav" />
          <span className="theme-preview-nav" />
        </span>
        <span className="theme-preview-main">
          <span className="theme-preview-bar" />
          <span className="theme-preview-page">
            <span className="theme-preview-title" />
            <span className="theme-preview-line" />
            <span className="theme-preview-line" />
            <span className="theme-preview-line is-short" />
            <span className="theme-preview-chips">
              <span className="theme-preview-chip is-accent" />
              <span className="theme-preview-chip is-green" />
              <span className="theme-preview-chip is-amber" />
              <span className="theme-preview-chip is-red" />
            </span>
          </span>
        </span>
      </span>
      <span className="theme-card-footer">
        <span className="theme-card-name">{theme.name}</span>
        {isDefault && <span className="theme-card-tag">Default</span>}
      </span>
    </button>
  );
}
