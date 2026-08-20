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

/**
 * The same, for a value that is either text or "nothing yet" — a transcript
 * that has not been produced, a file that was never picked.
 *
 * `null` is stored as the empty string, which is why an empty string cannot be
 * told apart from `null` on the way back. For the callers here that is the
 * same state, and the alternative (a JSON envelope) would put a parse step in
 * front of every read for no gain.
 */
export function usePersistentNullableText(
  key: string,
): [string | null, (value: string | null) => void] {
  const [raw, setRaw] = usePersistentState<string>(key, "");
  const set = useCallback(
    (value: string | null) => setRaw(value ?? ""),
    [setRaw],
  );
  return [raw === "" ? null : raw, set];
}
