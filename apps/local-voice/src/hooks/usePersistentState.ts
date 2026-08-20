import { useCallback, useEffect, useState } from "react";

const PREFIX = "lva.ui.";

/**
 * `useState` that survives an app restart.
 *
 * For UI state only — which page you were on, which tab was open. Real
 * settings live in the Tauri store (`useSettings`); putting view state there
 * too would mean a backend round trip for every click and a migration for
 * every new tab. The WebView's localStorage is per-app and persistent, which
 * is exactly the lifetime this state wants.
 *
 * A value that no longer maps to anything (a section that was renamed away)
 * is the caller's problem: validate what comes back before using it.
 */
export function usePersistentState<T extends string>(
  key: string,
  fallback: T,
  isValid?: (value: string) => boolean,
): [T, (value: T) => void] {
  const storageKey = `${PREFIX}${key}`;

  const [value, setValue] = useState<T>(() => {
    try {
      const stored = window.localStorage.getItem(storageKey);
      if (stored === null) return fallback;
      if (isValid && !isValid(stored)) return fallback;
      return stored as T;
    } catch {
      // Private mode / disabled storage — fall back to plain in-memory state.
      return fallback;
    }
  });

  useEffect(() => {
    try {
      window.localStorage.setItem(storageKey, value);
    } catch {
      /* not persisting is survivable; crashing the render is not */
    }
  }, [storageKey, value]);

  const set = useCallback((next: T) => setValue(next), []);

  return [value, set];
}
