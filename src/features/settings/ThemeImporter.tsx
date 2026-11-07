// Open VSX theme search and import (Settings > Appearance). Requests run in Rust and only start
// once the user opens the browser, so opening Settings never contacts Open VSX by itself.
import { useEffect, useMemo, useRef, useState } from "react";
import { ApiError } from "../../lib/api";
import { themesApi, type ThemeExtension } from "../../themes/api";
import { useThemes } from "../../themes/ThemeRoot";

const SEARCH_DELAY_MS = 350;
const MAX_QUERY_LENGTH = 100;

type Notice = { tone: "info" | "error"; text: string };

const compact = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });

function errorText(error: unknown): string {
  return error instanceof ApiError ? error.message : "Something went wrong. Try again.";
}

function extensionKey(namespace: string, name: string): string {
  return `${namespace}.${name}`.toLowerCase();
}

export function ThemeImporter() {
  const { themes } = useThemes();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ThemeExtension[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [importing, setImporting] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const request = useRef(0);

  const importedCount = useMemo(() => {
    const counts = new Map<string, number>();
    for (const theme of themes) {
      if (!theme.source) continue;
      const key = extensionKey(theme.source.namespace, theme.source.name);
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return counts;
  }, [themes]);

  useEffect(() => {
    if (!open) return;
    // Only the latest search may update the list; older responses are dropped.
    const id = ++request.current;
    const timer = window.setTimeout(
      () => {
        setSearching(true);
        themesApi
          .search(query)
          .then((found) => {
            if (request.current !== id) return;
            setResults(found);
            setSearchError(null);
          })
          .catch((error) => {
            if (request.current !== id) return;
            setResults([]);
            setSearchError(errorText(error));
          })
          .finally(() => request.current === id && setSearching(false));
      },
      query.trim() ? SEARCH_DELAY_MS : 0,
    );
    return () => window.clearTimeout(timer);
  }, [open, query]);

  const importExtension = (extension: ThemeExtension) => {
    const key = extensionKey(extension.namespace, extension.name);
    setImporting(key);
    setNotice(null);
    themesApi
      .importExtension(extension.namespace, extension.name)
      .then((added) => {
        const count = added.length === 1 ? "1 theme" : `${added.length} themes`;
        setNotice({
          tone: "info",
          text: `Imported ${count} from ${extension.displayName}. Choose them under Light theme or Dark theme.`,
        });
      })
      .catch((error) => setNotice({ tone: "error", text: errorText(error) }))
      .finally(() => setImporting(null));
  };

  return (
    <section className="appearance-group theme-import" aria-labelledby="theme-import-title">
      <div className="appearance-row">
        <div className="appearance-text">
          <h3 className="appearance-label" id="theme-import-title">
            Import from Open VSX
          </h3>
          <p className="appearance-hint">
            Search VS Code color themes on open-vsx.org. Every color theme in the extension is added to the pickers
            above.
          </p>
        </div>
        {!open && (
          <button type="button" className="btn btn-sm theme-import-button" onClick={() => setOpen(true)}>
            Browse themes
          </button>
        )}
      </div>

      {open && (
        <>
          <input
            type="search"
            className="theme-import-search"
            placeholder="Search themes, e.g. Nord or Catppuccin"
            aria-label="Search Open VSX themes"
            value={query}
            maxLength={MAX_QUERY_LENGTH}
            autoFocus
            onChange={(event) => setQuery(event.target.value)}
          />

          {notice && (
            <p className={`theme-import-notice is-${notice.tone}`} role={notice.tone === "error" ? "alert" : "status"}>
              {notice.text}
            </p>
          )}
          {searchError && (
            <p className="theme-import-notice is-error" role="alert">
              {searchError}
            </p>
          )}

          <p className="theme-import-caption" aria-live="polite">
            {searching
              ? "Searching Open VSX…"
              : results && !searchError
                ? results.length === 0
                  ? "No color themes found."
                  : query.trim()
                    ? `${results.length} results`
                    : "Most downloaded themes"
                : ""}
          </p>

          {results && results.length > 0 && (
            <ul className="theme-import-results" aria-busy={searching}>
              {results.map((extension) => {
                const key = extensionKey(extension.namespace, extension.name);
                const imported = importedCount.get(key) ?? 0;
                const busy = importing === key;
                return (
                  <li key={key} className="theme-import-row">
                    <div className="theme-import-info">
                      <span className="theme-import-name">
                        {extension.displayName}
                        {extension.verified && (
                          <span className="theme-import-verified" title="Verified publisher">
                            Verified
                          </span>
                        )}
                      </span>
                      <span className="theme-import-meta">
                        {extension.namespace} · {compact.format(extension.downloadCount)} downloads
                        {imported > 0 && ` · ${imported} imported`}
                      </span>
                      {extension.description && (
                        <span className="theme-import-description">{extension.description}</span>
                      )}
                    </div>
                    <button
                      type="button"
                      className="btn btn-sm theme-import-button"
                      disabled={importing !== null}
                      aria-label={`${imported > 0 ? "Reimport" : "Import"} ${extension.displayName}`}
                      onClick={() => importExtension(extension)}
                    >
                      {busy ? "Importing…" : imported > 0 ? "Reimport" : "Import"}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </>
      )}
    </section>
  );
}
