import { useCallback, useState } from "react";
import { storageKey, type FilterScope } from "./model";

function load<T>(key: string, parse: (raw: unknown) => T): T {
  try {
    const stored = localStorage.getItem(key);
    return parse(stored === null ? undefined : JSON.parse(stored));
  } catch {
    // Unavailable storage or a corrupted value: start from the defaults.
    return parse(undefined);
  }
}

function save(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Best effort: the filter still applies for this session.
  }
}

/**
 * Filter state remembered per organization in `localStorage`. Switching the organization
 * reloads that organization's filters. `parse` must validate untrusted stored values.
 */
export function usePersistentFilter<T>(
  organization: string,
  scope: FilterScope,
  parse: (raw: unknown) => T,
): [T, (next: T) => void] {
  const key = storageKey(organization, scope);
  const [state, setState] = useState(() => ({ key, value: load(key, parse) }));

  let current = state;
  if (state.key !== key) {
    current = { key, value: load(key, parse) };
    setState(current);
  }

  const update = useCallback(
    (next: T) => {
      save(key, next);
      setState({ key, value: next });
    },
    [key],
  );

  return [current.value, update];
}
