// Appearance settings: mode, the light/dark theme choices, translucency and Open VSX imports. Rendered by the Settings window.
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { ApiError } from "../../lib/api";
import type { AppearanceSettings, Settings, ThemeMode } from "../../lib/settings";
import { themesApi } from "../../themes/api";
import { useThemes } from "../../themes/ThemeRoot";
import { supportsTranslucency, themeCssVariables } from "../../themes/tokens";
import { DEFAULT_THEME_IDS, type Theme, type ThemeKind } from "../../themes/types";
import { ThemeImporter } from "./ThemeImporter";
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
          <p className="appearance-hint">Let the desktop show through the whole window. Off paints solid surfaces.</p>
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

      <GlassIntensity
        value={appearance.glassIntensity ?? 50}
        disabled={!appearance.translucency || !supportsTranslucency()}
        onCommit={(glassIntensity) => update({ glassIntensity })}
      />

      <ThemeImporter />
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
  const [removeError, setRemoveError] = useState<string | null>(null);
  const options = themes.filter((theme) => theme.kind === kind);
  // An unknown stored id paints the default, so show the default as selected.
  const effective = options.some((t) => t.id === selected) ? selected : DEFAULT_THEME_IDS[kind];
  const labelId = `appearance-${kind}-theme`;

  // Removing the selected theme needs no settings change: an unknown id paints the default of its kind.
  const remove = (theme: Theme) => {
    setRemoveError(null);
    themesApi.remove(theme.id).catch((error) => {
      setRemoveError(error instanceof ApiError ? error.message : "Could not remove the theme.");
    });
  };

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
            onRemove={theme.source ? () => remove(theme) : undefined}
          />
        ))}
      </div>
      {removeError && (
        <p className="theme-import-notice is-error" role="alert">
          {removeError}
        </p>
      )}
    </section>
  );
}

function ThemeCard({
  theme,
  selected,
  isDefault,
  onSelect,
  onRemove,
}: {
  theme: Theme;
  selected: boolean;
  isDefault: boolean;
  onSelect: () => void;
  /** Only for imported themes. */
  onRemove?: () => void;
}) {
  // The preview re-declares the theme's tokens locally, so it paints with that theme regardless of the active one.
  const previewVars = themeCssVariables(theme.colors) as CSSProperties;

  const source = theme.source;

  return (
    <div className="theme-card-slot">
      <button
        type="button"
        role="radio"
        aria-checked={selected}
        className={`theme-card${selected ? " is-selected" : ""}`}
        title={source ? `${source.displayName} (${source.namespace}.${source.name} ${source.version})` : undefined}
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
          {source && <span className="theme-card-tag is-imported">Open VSX</span>}
        </span>
      </button>
      {onRemove && (
        <button
          type="button"
          className="theme-card-remove"
          aria-label={`Remove ${theme.name}`}
          title="Remove theme"
          onClick={onRemove}
        >
          ×
        </button>
      )}
    </div>
  );
}

const GLASS_SAVE_DELAY_MS = 150;

/** Glass percentage slider. Moves instantly, persists debounced so dragging does not spam the settings file. */
function GlassIntensity({
  value,
  disabled,
  onCommit,
}: {
  value: number;
  disabled: boolean;
  onCommit: (value: number) => void;
}) {
  const [draft, setDraft] = useState(value);
  const timer = useRef<number | undefined>(undefined);

  // Follow external changes (another window, initial load) when not mid-drag.
  useEffect(() => {
    if (timer.current === undefined) setDraft(value);
  }, [value]);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const change = (next: number) => {
    setDraft(next);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = undefined;
      onCommit(next);
    }, GLASS_SAVE_DELAY_MS);
  };

  return (
    <section className={`appearance-row${disabled ? " is-disabled" : ""}`}>
      <div className="appearance-text">
        <h3 className="appearance-label" id="appearance-glass">
          Glass
        </h3>
        <p className="appearance-hint">
          {disabled
            ? "Available when translucency is on (macOS)."
            : "How much of the desktop shows through the window. Reading panes keep a higher opacity floor."}
        </p>
      </div>
      <div className="appearance-slider">
        <input
          type="range"
          min={0}
          max={100}
          step={5}
          value={draft}
          disabled={disabled}
          aria-labelledby="appearance-glass"
          aria-valuetext={`${draft}%`}
          onChange={(event) => change(Number(event.target.value))}
        />
        <output className="appearance-slider-value" aria-hidden>
          {draft}%
        </output>
      </div>
    </section>
  );
}
