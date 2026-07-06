// Theme logic kept pure so it unit-tests without a DOM.

export type Theme = "dark" | "light";

export const THEME_STORAGE_KEY = "jarvis.theme";

export function resolveInitialTheme(stored: string | null, prefersDark: boolean): Theme {
  if (stored === "dark" || stored === "light") return stored;
  return prefersDark ? "dark" : "light";
}

export function nextTheme(current: Theme): Theme {
  return current === "dark" ? "light" : "dark";
}
