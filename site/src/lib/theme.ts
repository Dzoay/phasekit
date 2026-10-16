// The viewer's theme: a mode (the system's, light or dark) and an accent. app.css draws both from the `data-theme` and
// `data-accent` attributes of <html>. The choice is kept in localStorage when the browser allows it; every access is
// guarded, so the page works without storage. index.html applies the stored choice before the first paint.

export const MODES = ["system", "light", "dark"] as const;
export const ACCENTS = ["teal", "indigo", "amber", "rose"] as const;
export type Mode = (typeof MODES)[number];
export type Accent = (typeof ACCENTS)[number];

export interface Theme {
  mode: Mode;
  accent: Accent;
}

export const MODE_LABELS: Record<Mode, string> = { system: "System", light: "Light", dark: "Dark" };
export const ACCENT_LABELS: Record<Accent, string> = { teal: "Teal", indigo: "Indigo", amber: "Amber", rose: "Rose" };

export const STORAGE_KEY = "phasekit.theme";
export const DEFAULT_THEME: Theme = { mode: "system", accent: "teal" };

const isMode = (v: unknown): v is Mode => MODES.some((m) => m === v);
const isAccent = (v: unknown): v is Accent => ACCENTS.some((a) => a === v);

/** The theme a stored string names, or the default for anything else. */
export function parseTheme(raw: string | null): Theme {
  try {
    const value: unknown = JSON.parse(raw ?? "null");
    if (typeof value === "object" && value !== null) {
      const { mode, accent } = value as Record<string, unknown>;
      if (isMode(mode) && isAccent(accent)) return { mode, accent };
    }
  } catch {
    // Not JSON: the default.
  }
  return DEFAULT_THEME;
}

/** The stored theme, or the default where storage is empty or blocked. */
export function loadTheme(storage: () => Storage): Theme {
  try {
    return parseTheme(storage().getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_THEME;
  }
}

/** Store the theme; false where the browser refuses. */
export function saveTheme(theme: Theme, storage: () => Storage): boolean {
  try {
    storage().setItem(STORAGE_KEY, JSON.stringify(theme));
    return true;
  } catch {
    return false;
  }
}

export function applyTheme(theme: Theme, root: HTMLElement): void {
  root.dataset.theme = theme.mode;
  root.dataset.accent = theme.accent;
}
